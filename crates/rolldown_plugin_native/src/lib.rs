//! Dynamic loader and `rolldown_plugin::Plugin` adapter for the experimental
//! native plugin ABI.
//!
//! See `internal-docs/native-plugin-abi/implementation.md` for the ownership
//! and unload invariants.

#![deny(clippy::undocumented_unsafe_blocks)]

use std::{
  borrow::Cow,
  ffi::c_void,
  path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use libloading::Library;

use rolldown_common::{ImportKind, ModuleType, ResolvedExternal, side_effects::HookSideEffects};
use rolldown_plugin::{
  HookBuildEndArgs, HookBuildStartArgs, HookLoadArgs, HookLoadOutput, HookResolveIdArgs,
  HookResolveIdOutput, HookTransformArgs, HookTransformOutput, HookTransformOutputMap, HookUsage,
  LoadPluginContext, Plugin, PluginContext, SharedLoadPluginContext, SharedTransformPluginContext,
  TransformPluginContext,
};
use rolldown_plugin_native_abi::{
  ABI_VERSION, EXTERNAL_ABSOLUTE, EXTERNAL_FALSE, EXTERNAL_NONE, EXTERNAL_RELATIVE, EXTERNAL_TRUE,
  HookUsage as NativeHookUsage, IMPORT_KIND_AT_IMPORT, IMPORT_KIND_DYNAMIC_IMPORT,
  IMPORT_KIND_HOT_ACCEPT, IMPORT_KIND_IMPORT, IMPORT_KIND_NEW_URL, IMPORT_KIND_REQUIRE,
  IMPORT_KIND_URL_IMPORT, INIT_SYMBOL, SIDE_EFFECTS_FALSE, SIDE_EFFECTS_NO_TREESHAKE,
  SIDE_EFFECTS_NONE, SIDE_EFFECTS_TRUE, STATUS_ERROR, STATUS_INCOMPATIBLE, STATUS_OK, ffi,
};
use rolldown_sourcemap::{SourceMap, SourceMapRef};

mod manifest;

static HOST_API: ffi::HostApi = ffi::HostApi {
  header: ffi::Header { abi_version: ABI_VERSION, size: size_of::<ffi::HostApi>() },
  buffer_alloc: Some(buffer_alloc),
  add_watch_file: Some(add_watch_file),
};

#[derive(Clone, Copy)]
enum HostContext<'a> {
  Plugin(&'a PluginContext),
  Load(&'a LoadPluginContext),
  Transform(&'a TransformPluginContext),
}

impl<'a> HostContext<'a> {
  fn add_watch_file(self, path: &str) {
    match self {
      Self::Plugin(context) => context.add_watch_file(path),
      Self::Load(context) => context.add_watch_file(path),
      Self::Transform(context) => context.add_watch_file(path),
    }
  }

  fn cwd(self) -> Result<&'a str> {
    let cwd = match self {
      Self::Plugin(context) => context.cwd(),
      Self::Load(context) => context.cwd(),
      Self::Transform(context) => context.cwd(),
    };
    cwd.to_str().context("native plugins require the build cwd to be valid UTF-8")
  }
}

/// A loaded shared library adapted to Rolldown's internal plugin interface.
pub struct ExternalNativePlugin {
  descriptor: ffi::PluginDescriptor,
  name: String,
  // Must remain alive until after `descriptor.deinit` has returned.
  _library: Library,
}

impl std::fmt::Debug for ExternalNativePlugin {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("ExternalNativePlugin")
      .field("name", &self.name)
      .field("hook_usage", &self.descriptor.hook_usage)
      .finish_non_exhaustive()
  }
}

// ABI v1 requires plugins to support concurrent calls. The C ABI cannot encode
// auto traits, so this remains a trusted boundary for non-Rust plugins.
// SAFETY: ABI v1 requires plugin state and every hook to permit concurrent calls.
unsafe impl Send for ExternalNativePlugin {}

// SAFETY: ABI v1 requires plugin state and every hook to permit concurrent calls.
unsafe impl Sync for ExternalNativePlugin {}

impl ExternalNativePlugin {
  pub fn load(source: &str, cwd: Option<&Path>, config: &str) -> Result<Self> {
    let path = resolve_library_path(source, cwd)?;
    if !path.is_absolute() {
      bail!("native plugin library path must be absolute: {}", path.display());
    }

    let path = path
      .canonicalize()
      .with_context(|| format!("failed to resolve native plugin library {}", path.display()))?;
    if !path
      .metadata()
      .with_context(|| format!("failed to inspect native plugin library {}", path.display()))?
      .is_file()
    {
      bail!("native plugin library is not a regular file: {}", path.display());
    }

    // SAFETY: Opening an explicitly configured native library is the purpose of
    // this API. It is trusted in-process code, as documented by the JS API.
    let library = unsafe { load_library(&path) }
      .with_context(|| format!("failed to open native plugin library {}", path.display()))?;

    // SAFETY: The symbol is validated through the versioned C ABI immediately
    // after lookup and cannot outlive `library` because it is copied.
    let init = unsafe { library.get::<ffi::InitFn>(INIT_SYMBOL) }.with_context(|| {
      format!("native plugin library {} does not export rolldown_plugin_init", path.display())
    })?;
    let init = *init;
    let args = ffi::InitArgs {
      header: ffi::Header::new::<ffi::InitArgs>(),
      host: &raw const HOST_API,
      config: ffi::StringView::new(config),
    };
    let mut descriptor = ffi::PluginDescriptor::default();
    let mut error = ffi::Buffer::default();

    // SAFETY: All pointers refer to ABI v1 values that live through the call.
    // Any returned error buffer comes from HOST_API and is reclaimed exactly
    // once, after the plugin has finished writing it.
    let (status, error) = unsafe {
      let status = init(&raw const args, &raw mut descriptor, &raw mut error);
      let error = take_error(&mut error);
      (status, error)
    };

    if let Err(error) = check_init_status(status, error) {
      // An implementation may have created state before discovering an
      // initialization error. Honor its destructor before unloading.
      deinit_descriptor(&mut descriptor);
      return Err(error);
    }

    if let Err(error) = validate_descriptor(&descriptor) {
      deinit_descriptor(&mut descriptor);
      return Err(error);
    }

    // SAFETY: The descriptor name is borrowed from the loaded library or its
    // state and is copied before either can be released.
    let name = match unsafe { view_as_str(descriptor.name) }
      .context("native plugin name is not valid UTF-8")
    {
      Ok(name) => name.to_string(),
      Err(error) => {
        deinit_descriptor(&mut descriptor);
        return Err(error);
      }
    };
    if name.is_empty() {
      deinit_descriptor(&mut descriptor);
      bail!("native plugin name must not be empty");
    }
    Ok(Self { descriptor, name, _library: library })
  }
}

fn resolve_library_path(source: &str, cwd: Option<&Path>) -> Result<PathBuf> {
  let path = if source.get(..5).is_some_and(|prefix| prefix.eq_ignore_ascii_case("file:")) {
    let url = url::Url::parse(source).context("native plugin library is not a valid file URL")?;
    if url.host_str().is_some() || url.query().is_some() || url.fragment().is_some() {
      bail!("native plugin library file URL must not contain a host, query, or fragment");
    }
    url
      .to_file_path()
      .map_err(|()| anyhow::anyhow!("native plugin library URL must point to a local file"))?
  } else {
    if source.contains("://") {
      bail!("native plugin source must be a package name, absolute path, or file URL");
    }
    let path = PathBuf::from(source);
    if path.is_absolute() {
      path
    } else {
      return manifest::resolve_package_library(
        source,
        cwd.context("native plugin package resolution requires the build cwd")?,
      );
    }
  };

  with_platform_library_name(path)
}

fn with_platform_library_name(mut path: PathBuf) -> Result<PathBuf> {
  if path.extension().is_none() {
    let name = path
      .file_name()
      .and_then(|name| name.to_str())
      .context("native plugin library name must be valid UTF-8")?;
    let prefix = std::env::consts::DLL_PREFIX;
    let name = if prefix.is_empty() || name.starts_with(prefix) {
      name.to_string()
    } else {
      format!("{prefix}{name}")
    };
    path.set_file_name(format!("{name}{}", std::env::consts::DLL_SUFFIX));
  }
  Ok(path)
}

#[cfg(unix)]
unsafe fn load_library(path: &Path) -> Result<Library, libloading::Error> {
  use libloading::os::unix::{Library as UnixLibrary, RTLD_LOCAL, RTLD_NOW};

  // RTLD_NOW rejects missing symbols before any hook can run. RTLD_LOCAL keeps
  // plugin symbols out of the process-wide namespace and avoids accidental
  // interposition between independently distributed plugins.
  // SAFETY: Loading the explicitly configured library is this API's trust
  // boundary; constructors and destructors are permitted to run native code.
  unsafe { UnixLibrary::open(Some(path), RTLD_NOW | RTLD_LOCAL) }.map(Into::into)
}

#[cfg(windows)]
unsafe fn load_library(path: &Path) -> Result<Library, libloading::Error> {
  use libloading::os::windows::{
    LOAD_LIBRARY_SEARCH_DEFAULT_DIRS, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR, Library as WindowsLibrary,
  };

  // Restrict dependency lookup to the plugin directory and Windows' safe
  // default directories. In particular, do not search the current directory.
  // SAFETY: Loading the explicitly configured library is this API's trust
  // boundary; constructors and destructors are permitted to run native code.
  unsafe {
    WindowsLibrary::load_with_flags(
      path,
      LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_DEFAULT_DIRS,
    )
  }
  .map(Into::into)
}

impl Drop for ExternalNativePlugin {
  fn drop(&mut self) {
    deinit_descriptor(&mut self.descriptor);
  }
}

fn deinit_descriptor(descriptor: &mut ffi::PluginDescriptor) {
  if let Some(deinit) = descriptor.deinit.take() {
    // SAFETY: The state belongs to this descriptor and deinit runs exactly
    // once while the dynamic library is still loaded.
    unsafe { deinit(descriptor.state) };
    descriptor.state = std::ptr::null_mut();
  }
}

impl Plugin for ExternalNativePlugin {
  fn name(&self) -> Cow<'static, str> {
    Cow::Owned(self.name.clone())
  }

  async fn build_start(
    &self,
    ctx: &PluginContext,
    _args: &HookBuildStartArgs<'_>,
  ) -> rolldown_plugin::HookNoopReturn {
    let Some(hook) = self.descriptor.build_start else { return Ok(()) };
    let host_context = HostContext::Plugin(ctx);
    let context = raw_context(&host_context)?;
    let args = ffi::BuildStartArgs { header: ffi::Header::new::<ffi::BuildStartArgs>(), context };
    call_no_output(self.descriptor.state, hook, &args, "buildStart")
  }

  async fn resolve_id(
    &self,
    ctx: &PluginContext,
    args: &HookResolveIdArgs<'_>,
  ) -> rolldown_plugin::HookResolveIdReturn {
    let Some(hook) = self.descriptor.resolve_id else { return Ok(None) };
    let host_context = HostContext::Plugin(ctx);
    let raw_args = ffi::ResolveIdArgs {
      header: ffi::Header::new::<ffi::ResolveIdArgs>(),
      context: raw_context(&host_context)?,
      specifier: ffi::StringView::new(args.specifier),
      importer: ffi::OptionalStringView::new(args.importer),
      kind: encode_import_kind(args.kind),
      is_entry: u8::from(args.is_entry),
      reserved: [0; 6],
    };
    let mut output = ffi::ResolveIdOutput {
      header: ffi::Header::new::<ffi::ResolveIdOutput>(),
      ..ffi::ResolveIdOutput::default()
    };
    let mut error = ffi::Buffer::default();

    // SAFETY: The validated hook receives ABI v1 values that remain live for
    // the call. Returned buffers come from HOST_API and are reclaimed exactly
    // once, after the plugin has finished writing them.
    let (status, error, id) = unsafe {
      let status =
        hook(self.descriptor.state, &raw const raw_args, &raw mut output, &raw mut error);
      let error = take_error(&mut error);
      let id = take_string(&mut output.id);
      (status, error, id)
    };

    check_status(status, error, "resolveId")?;
    validate_output_header::<ffi::ResolveIdOutput>(&output.header, "resolveId")?;
    validate_boolean(output.handled, "resolveId handled")?;
    if output.handled == 0 {
      return Ok(None);
    }

    let id = id.context("resolveId returned an invalid output buffer or non-UTF-8 id")?;

    Ok(Some(HookResolveIdOutput {
      id: id.into(),
      external: decode_external(output.external)?,
      normalize_external_id: None,
      side_effects: decode_side_effects(output.side_effects)?,
      package_json_path: None,
      attributes: None,
    }))
  }

  async fn load(
    &self,
    ctx: SharedLoadPluginContext,
    args: &HookLoadArgs<'_>,
  ) -> rolldown_plugin::HookLoadReturn {
    let Some(hook) = self.descriptor.load else { return Ok(None) };
    let host_context = HostContext::Load(ctx.as_ref());
    let raw_args = ffi::LoadArgs {
      header: ffi::Header::new::<ffi::LoadArgs>(),
      context: raw_context(&host_context)?,
      id: ffi::StringView::new(args.id),
    };
    let mut output = ffi::LoadOutput {
      header: ffi::Header::new::<ffi::LoadOutput>(),
      ..ffi::LoadOutput::default()
    };
    let mut error = ffi::Buffer::default();

    // SAFETY: The validated hook receives ABI v1 values that remain live for
    // the call. Returned buffers come from HOST_API and are all reclaimed
    // exactly once, including partial output from a failed hook.
    let (status, error, code, map, module_type) = unsafe {
      let status =
        hook(self.descriptor.state, &raw const raw_args, &raw mut output, &raw mut error);
      let error = take_error(&mut error);
      let code = take_string(&mut output.code);
      let map = take_optional_string(&mut output.map, "load sourcemap");
      let module_type = take_optional_string(&mut output.module_type, "load module type");
      (status, error, code, map, module_type)
    };

    check_status(status, error, "load")?;
    validate_output_header::<ffi::LoadOutput>(&output.header, "load")?;
    validate_boolean(output.handled, "load handled")?;
    if output.handled == 0 {
      return Ok(None);
    }

    let code = code.context("load returned an invalid output buffer or non-UTF-8 code")?;
    let map = parse_sourcemap(map?, "load")?;
    let module_type = module_type?.map(ModuleType::from_str_with_fallback);

    Ok(Some(HookLoadOutput {
      code: code.into(),
      map,
      side_effects: decode_side_effects(output.side_effects)?,
      module_type,
    }))
  }

  async fn transform(
    &self,
    ctx: SharedTransformPluginContext,
    args: &HookTransformArgs<'_>,
  ) -> rolldown_plugin::HookTransformReturn {
    let Some(hook) = self.descriptor.transform else { return Ok(None) };
    let host_context = HostContext::Transform(ctx.as_ref());
    let module_type = args.module_type.to_string();
    let raw_args = ffi::TransformArgs {
      header: ffi::Header::new::<ffi::TransformArgs>(),
      context: raw_context(&host_context)?,
      id: ffi::StringView::new(args.id),
      code: ffi::StringView::new(args.code),
      module_type: ffi::StringView::new(&module_type),
    };
    let mut output = ffi::TransformOutput {
      header: ffi::Header::new::<ffi::TransformOutput>(),
      ..ffi::TransformOutput::default()
    };
    let mut error = ffi::Buffer::default();

    // SAFETY: The validated hook receives ABI v1 values that remain live for
    // the call. Returned buffers come from HOST_API and are all reclaimed
    // exactly once, including partial output from a failed hook.
    let (status, error, code, source_map, module_type) = unsafe {
      let status =
        hook(self.descriptor.state, &raw const raw_args, &raw mut output, &raw mut error);
      let error = take_error(&mut error);
      let code = take_optional_string(&mut output.code, "transform code");
      let source_map = take_optional_string(&mut output.map, "transform sourcemap");
      let module_type = take_optional_string(&mut output.module_type, "transform module type");
      (status, error, code, source_map, module_type)
    };

    check_status(status, error, "transform")?;
    validate_output_header::<ffi::TransformOutput>(&output.header, "transform")?;
    validate_boolean(output.handled, "transform handled")?;
    if output.handled == 0 {
      return Ok(None);
    }

    let code = code?;
    let map = match parse_sourcemap(source_map?, "transform")? {
      Some(map) => HookTransformOutputMap::Sourcemap(Box::new(map)),
      None => HookTransformOutputMap::Omitted,
    };
    let module_type = module_type?.map(ModuleType::from_str_with_fallback);

    Ok(Some(HookTransformOutput {
      code,
      map,
      side_effects: decode_side_effects(output.side_effects)?,
      module_type,
    }))
  }

  async fn build_end(
    &self,
    ctx: &PluginContext,
    args: Option<&HookBuildEndArgs<'_>>,
  ) -> rolldown_plugin::HookNoopReturn {
    let Some(hook) = self.descriptor.build_end else { return Ok(()) };
    let host_context = HostContext::Plugin(ctx);
    let raw_args = ffi::BuildEndArgs {
      header: ffi::Header::new::<ffi::BuildEndArgs>(),
      context: raw_context(&host_context)?,
      error_count: args.map_or(0, |args| args.errors.len()),
    };
    call_no_output(self.descriptor.state, hook, &raw_args, "buildEnd")
  }

  fn register_hook_usage(&self) -> HookUsage {
    let usage = NativeHookUsage::from_bits_truncate(self.descriptor.hook_usage);
    let mut result = HookUsage::empty();

    for (hook, native_hook) in [
      (HookUsage::BuildStart, NativeHookUsage::BUILD_START),
      (HookUsage::ResolveId, NativeHookUsage::RESOLVE_ID),
      (HookUsage::Load, NativeHookUsage::LOAD),
      (HookUsage::Transform, NativeHookUsage::TRANSFORM),
      (HookUsage::BuildEnd, NativeHookUsage::BUILD_END),
    ] {
      result.set(hook, usage.contains(native_hook));
    }

    result
  }
}

fn validate_descriptor(descriptor: &ffi::PluginDescriptor) -> Result<()> {
  if descriptor.header.abi_version != ABI_VERSION {
    bail!(
      "native plugin descriptor uses ABI version {}, but Rolldown implements {ABI_VERSION}",
      descriptor.header.abi_version
    );
  }
  if descriptor.header.size < size_of::<ffi::PluginDescriptor>() {
    bail!("native plugin descriptor is smaller than ABI v1 requires");
  }
  let usage = NativeHookUsage::from_bits(descriptor.hook_usage)
    .context("native plugin descriptor contains unknown hook bits")?;

  let hooks = [
    ("buildStart", NativeHookUsage::BUILD_START, descriptor.build_start.is_some()),
    ("resolveId", NativeHookUsage::RESOLVE_ID, descriptor.resolve_id.is_some()),
    ("load", NativeHookUsage::LOAD, descriptor.load.is_some()),
    ("transform", NativeHookUsage::TRANSFORM, descriptor.transform.is_some()),
    ("buildEnd", NativeHookUsage::BUILD_END, descriptor.build_end.is_some()),
  ];
  for (name, hook, present) in hooks {
    if usage.contains(hook) && !present {
      bail!("native plugin enables {name} but provides no function pointer");
    }
  }

  if !descriptor.state.is_null() && descriptor.deinit.is_none() {
    bail!("native plugin provides state but no deinit function");
  }
  Ok(())
}

fn raw_context(host_context: &HostContext<'_>) -> Result<ffi::HookContext> {
  Ok(ffi::HookContext {
    header: ffi::Header::new::<ffi::HookContext>(),
    host: &raw const HOST_API,
    handle: std::ptr::from_ref(host_context).cast_mut().cast(),
    cwd: ffi::StringView::new(host_context.cwd()?),
  })
}

fn call_no_output<A>(
  state: *mut c_void,
  hook: unsafe extern "C" fn(*mut c_void, *const A, *mut ffi::Buffer) -> u8,
  args: &A,
  hook_name: &str,
) -> Result<()> {
  let mut error = ffi::Buffer::default();

  // SAFETY: The validated hook receives args that remain live for the call.
  // Any returned error buffer comes from HOST_API and is reclaimed exactly
  // once, after the plugin has finished writing it.
  let (status, error) = unsafe {
    let status = hook(state, std::ptr::from_ref(args), &raw mut error);
    let error = take_error(&mut error);
    (status, error)
  };

  check_status(status, error, hook_name)
}

fn check_init_status(status: u8, error: Result<String>) -> Result<()> {
  let message = error?;
  match status {
    STATUS_OK => Ok(()),
    STATUS_ERROR if message.is_empty() => bail!("native plugin initialization failed"),
    STATUS_ERROR => bail!("native plugin initialization failed: {message}"),
    STATUS_INCOMPATIBLE => {
      bail!("native plugin is incompatible with Rolldown ABI version {ABI_VERSION}")
    }
    status => bail!("native plugin returned unknown initialization status {status}"),
  }
}

fn check_status(status: u8, error: Result<String>, hook_name: &str) -> Result<()> {
  let message = error?;
  match status {
    STATUS_OK => Ok(()),
    STATUS_ERROR if message.is_empty() => bail!("native plugin {hook_name} hook failed"),
    STATUS_ERROR => bail!("native plugin {hook_name} hook failed: {message}"),
    STATUS_INCOMPATIBLE => bail!("native plugin reported an ABI mismatch in {hook_name}"),
    status => bail!("native plugin {hook_name} hook returned unknown status {status}"),
  }
}

unsafe fn take_optional_string(
  output: &mut ffi::OptionalBuffer,
  field: &str,
) -> Result<Option<String>> {
  // SAFETY: The caller transfers a not-yet-consumed host allocation.
  let value = unsafe { take_string(&mut output.value) }?;
  validate_boolean(output.present, field)?;
  Ok((output.present != 0).then_some(value))
}

fn parse_sourcemap(map: Option<String>, hook_name: &str) -> Result<Option<SourceMap>> {
  map
    .map(|map| {
      SourceMapRef::from_json_string(&map)
        .map(SourceMapRef::into_owned)
        .with_context(|| format!("native plugin {hook_name} hook returned an invalid sourcemap"))
    })
    .transpose()
}

fn validate_output_header<T>(header: &ffi::Header, hook_name: &str) -> Result<()> {
  if header.abi_version != ABI_VERSION || header.size < size_of::<T>() {
    bail!("native plugin {hook_name} hook corrupted its output header");
  }
  Ok(())
}

fn validate_boolean(value: u8, field: &str) -> Result<()> {
  if value > 1 {
    bail!("native plugin returned invalid boolean {value} for {field}");
  }
  Ok(())
}

fn decode_external(value: u8) -> Result<Option<ResolvedExternal>> {
  Ok(match value {
    EXTERNAL_NONE => None,
    EXTERNAL_FALSE => Some(ResolvedExternal::Bool(false)),
    EXTERNAL_TRUE => Some(ResolvedExternal::Bool(true)),
    EXTERNAL_ABSOLUTE => Some(ResolvedExternal::Absolute),
    EXTERNAL_RELATIVE => Some(ResolvedExternal::Relative),
    _ => bail!("native plugin returned invalid external mode {value}"),
  })
}

fn decode_side_effects(value: u8) -> Result<Option<HookSideEffects>> {
  Ok(match value {
    SIDE_EFFECTS_NONE => None,
    SIDE_EFFECTS_TRUE => Some(HookSideEffects::True),
    SIDE_EFFECTS_FALSE => Some(HookSideEffects::False),
    SIDE_EFFECTS_NO_TREESHAKE => Some(HookSideEffects::NoTreeshake),
    _ => bail!("native plugin returned invalid side-effects mode {value}"),
  })
}

fn encode_import_kind(kind: ImportKind) -> u8 {
  match kind {
    ImportKind::Import => IMPORT_KIND_IMPORT,
    ImportKind::DynamicImport => IMPORT_KIND_DYNAMIC_IMPORT,
    ImportKind::Require => IMPORT_KIND_REQUIRE,
    ImportKind::AtImport => IMPORT_KIND_AT_IMPORT,
    ImportKind::UrlImport => IMPORT_KIND_URL_IMPORT,
    ImportKind::NewUrl => IMPORT_KIND_NEW_URL,
    ImportKind::HotAccept => IMPORT_KIND_HOT_ACCEPT,
  }
}

unsafe extern "C" fn buffer_alloc(len: usize, output: *mut ffi::Buffer) -> u8 {
  // SAFETY: A conforming plugin passes a writable Buffer output slot.
  let Some(output) = (unsafe { output.as_mut() }) else {
    return STATUS_ERROR;
  };
  if len == 0 {
    *output = ffi::Buffer::default();
    return STATUS_OK;
  }
  let mut bytes = Vec::<u8>::new();
  if bytes.try_reserve_exact(len).is_err() {
    *output = ffi::Buffer::default();
    return STATUS_ERROR;
  }
  output.data = bytes.as_mut_ptr();
  output.len = len;
  output.capacity = bytes.capacity();
  std::mem::forget(bytes);
  STATUS_OK
}

unsafe extern "C" fn add_watch_file(context: *mut c_void, path: ffi::StringView) -> u8 {
  // SAFETY: raw_context created this handle from a live HostContext and keeps
  // it alive for the synchronous hook call.
  let context = unsafe { context.cast::<HostContext<'_>>().as_ref() };
  // SAFETY: The plugin borrows path only for this callback invocation.
  let path = unsafe { view_as_str(path) };
  let (Some(context), Ok(path)) = (context, path) else {
    return STATUS_ERROR;
  };
  std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| context.add_watch_file(path)))
    .map_or(STATUS_ERROR, |()| STATUS_OK)
}

unsafe fn take_error(buffer: &mut ffi::Buffer) -> Result<String> {
  // SAFETY: The caller transfers a not-yet-consumed host allocation.
  unsafe { take_string(buffer) }.context("native plugin returned an invalid error buffer")
}

unsafe fn take_string(buffer: &mut ffi::Buffer) -> Result<String> {
  // SAFETY: The caller transfers a not-yet-consumed host allocation.
  let bytes = unsafe { take_buffer(buffer) }?;
  String::from_utf8(bytes).context("native plugin returned non-UTF-8 text")
}

unsafe fn take_buffer(buffer: &mut ffi::Buffer) -> Result<Vec<u8>> {
  if buffer.data.is_null() && buffer.len == 0 && buffer.capacity == 0 {
    *buffer = ffi::Buffer::default();
    return Ok(Vec::new());
  }
  if buffer.data.is_null() || buffer.capacity < buffer.len {
    // We cannot safely reclaim a buffer whose opaque ownership fields were
    // corrupted. Native plugins are trusted code; reject it without
    // dereferencing the pointer or constructing an invalid Vec.
    *buffer = ffi::Buffer::default();
    bail!("native plugin returned an invalid host-allocated buffer");
  }
  // SAFETY: buffer_alloc created this Vec allocation and the ABI requires the
  // plugin to preserve data/capacity. The length bound was checked above.
  let bytes = unsafe { Vec::from_raw_parts(buffer.data, buffer.len, buffer.capacity) };
  *buffer = ffi::Buffer::default();
  Ok(bytes)
}

unsafe fn view_as_str<'a>(view: ffi::StringView) -> Result<&'a str> {
  let bytes = if view.len == 0 {
    &[]
  } else {
    if view.data.is_null() {
      bail!("native plugin returned a null string pointer with a non-zero length");
    }
    // SAFETY: The ABI requires data to remain readable for len bytes during
    // the synchronous call. The null case was rejected above.
    unsafe { std::slice::from_raw_parts(view.data, view.len) }
  };
  std::str::from_utf8(bytes).context("native plugin returned non-UTF-8 text")
}

#[cfg(test)]
mod tests {
  use std::sync::atomic::{AtomicUsize, Ordering};

  use super::*;

  static DEINIT_CALLS: AtomicUsize = AtomicUsize::new(0);

  unsafe extern "C" fn count_deinit(_state: *mut c_void) {
    DEINIT_CALLS.fetch_add(1, Ordering::Relaxed);
  }

  #[test]
  fn host_buffers_round_trip_utf8() {
    let mut buffer = ffi::Buffer::default();
    // SAFETY: buffer is a writable output slot for this host allocator test.
    assert_eq!(unsafe { buffer_alloc(6, &raw mut buffer) }, STATUS_OK);
    // SAFETY: buffer_alloc returned at least six writable bytes.
    unsafe { std::ptr::copy_nonoverlapping("native".as_ptr(), buffer.data, 6) };
    // SAFETY: buffer is the live, not-yet-consumed host allocation above.
    assert_eq!(unsafe { take_string(&mut buffer) }.unwrap(), "native");
    assert!(buffer.data.is_null());
    assert_eq!(buffer.capacity, 0);
  }

  #[test]
  fn host_buffers_reject_corrupted_ownership_fields_without_dereferencing() {
    let mut buffer =
      ffi::Buffer { data: std::ptr::NonNull::<u8>::dangling().as_ptr(), len: 2, capacity: 1 };
    // SAFETY: take_buffer rejects capacity < len before dereferencing data.
    let error = unsafe { take_buffer(&mut buffer) }.unwrap_err();
    assert!(error.to_string().contains("invalid host-allocated buffer"));
    assert!(buffer.data.is_null());
  }

  #[test]
  fn host_buffer_allocation_failure_does_not_unwind_across_ffi() {
    let mut buffer =
      ffi::Buffer { data: std::ptr::NonNull::<u8>::dangling().as_ptr(), len: 1, capacity: 1 };
    // SAFETY: buffer is a writable output slot and usize::MAX intentionally
    // exercises the allocator's capacity-overflow path.
    assert_eq!(unsafe { buffer_alloc(usize::MAX, &raw mut buffer) }, STATUS_ERROR);
    assert!(buffer.data.is_null());
    assert_eq!(buffer.len, 0);
    assert_eq!(buffer.capacity, 0);
  }

  #[test]
  fn descriptor_deinitialization_is_exactly_once() {
    DEINIT_CALLS.store(0, Ordering::Relaxed);
    let mut descriptor = ffi::PluginDescriptor {
      state: std::ptr::NonNull::<u8>::dangling().as_ptr().cast(),
      deinit: Some(count_deinit),
      ..ffi::PluginDescriptor::default()
    };
    deinit_descriptor(&mut descriptor);
    deinit_descriptor(&mut descriptor);
    assert_eq!(DEINIT_CALLS.load(Ordering::Relaxed), 1);
    assert!(descriptor.state.is_null());
  }

  #[test]
  fn invalid_result_discriminants_are_rejected() {
    decode_external(255).unwrap_err();
    decode_side_effects(255).unwrap_err();
  }

  #[test]
  fn package_sources_require_the_build_cwd() {
    let error = ExternalNativePlugin::load("plugin.so", None, "{}").unwrap_err();
    assert!(error.to_string().contains("requires the build cwd"));
  }

  #[test]
  fn extensionless_library_names_use_the_platform_convention() {
    let path = resolve_library_path("/tmp/native_plugin", None).unwrap();
    assert_eq!(
      path.file_name().unwrap(),
      format!("{}native_plugin{}", std::env::consts::DLL_PREFIX, std::env::consts::DLL_SUFFIX)
        .as_str()
    );
  }

  #[test]
  fn already_prefixed_library_names_are_not_prefixed_twice() {
    let path =
      resolve_library_path(&format!("/tmp/{}native_plugin", std::env::consts::DLL_PREFIX), None)
        .unwrap();
    assert_eq!(
      path.file_name().unwrap(),
      format!("{}native_plugin{}", std::env::consts::DLL_PREFIX, std::env::consts::DLL_SUFFIX)
        .as_str()
    );
  }

  #[test]
  fn local_file_urls_are_supported() {
    let url = url::Url::from_file_path("/tmp/native plugin").unwrap();
    let path = resolve_library_path(url.as_str(), None).unwrap();
    assert_eq!(
      path.file_name().unwrap(),
      format!("{}native plugin{}", std::env::consts::DLL_PREFIX, std::env::consts::DLL_SUFFIX)
        .as_str()
    );
  }

  #[test]
  fn remote_and_ambiguous_urls_are_rejected() {
    assert!(
      resolve_library_path("https://example.com/plugin.so", None)
        .unwrap_err()
        .to_string()
        .contains("package name, absolute path, or file URL")
    );
    assert!(
      resolve_library_path("file://example.com/plugin.so", None)
        .unwrap_err()
        .to_string()
        .contains("must not contain")
    );
    assert!(
      resolve_library_path("file:///tmp/plugin.so?version=1", None)
        .unwrap_err()
        .to_string()
        .contains("must not contain")
    );
  }

  #[test]
  fn loader_rejects_directories_and_non_libraries_before_initialization() {
    let root = std::env::temp_dir()
      .join(format!("rolldown-native-plugin-loader-test-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();

    let directory = root.join(format!("directory{}", std::env::consts::DLL_SUFFIX));
    std::fs::create_dir(&directory).unwrap();
    let error = ExternalNativePlugin::load(directory.to_str().unwrap(), None, "{}").unwrap_err();
    assert!(error.to_string().contains("not a regular file"));

    let invalid_library = root.join(format!("invalid{}", std::env::consts::DLL_SUFFIX));
    std::fs::write(&invalid_library, b"not a dynamic library").unwrap();
    let error =
      ExternalNativePlugin::load(invalid_library.to_str().unwrap(), None, "{}").unwrap_err();
    assert!(error.to_string().contains("failed to open native plugin library"));

    std::fs::remove_dir_all(root).unwrap();
  }

  #[test]
  fn descriptor_validation_rejects_incompatible_state_and_hooks() {
    let default = ffi::PluginDescriptor::default();
    let descriptor = ffi::PluginDescriptor {
      header: ffi::Header {
        abi_version: default.header.abi_version + 1,
        size: default.header.size,
      },
      ..default
    };
    validate_descriptor(&descriptor).unwrap_err();

    let descriptor = ffi::PluginDescriptor {
      hook_usage: NativeHookUsage::TRANSFORM.bits(),
      ..ffi::PluginDescriptor::default()
    };
    assert!(validate_descriptor(&descriptor).unwrap_err().to_string().contains("transform"));

    let descriptor = ffi::PluginDescriptor {
      state: std::ptr::NonNull::<u8>::dangling().as_ptr().cast(),
      ..ffi::PluginDescriptor::default()
    };
    assert!(validate_descriptor(&descriptor).unwrap_err().to_string().contains("deinit"));

    let descriptor =
      ffi::PluginDescriptor { hook_usage: 1 << 63, ..ffi::PluginDescriptor::default() };
    assert!(validate_descriptor(&descriptor).unwrap_err().to_string().contains("unknown hook"));
  }

  #[test]
  fn output_validation_rejects_corrupted_headers_and_booleans() {
    let mut header = ffi::Header::new::<ffi::TransformOutput>();
    header.abi_version += 1;
    validate_output_header::<ffi::TransformOutput>(&header, "transform").unwrap_err();
    validate_boolean(2, "handled").unwrap_err();
  }
}
