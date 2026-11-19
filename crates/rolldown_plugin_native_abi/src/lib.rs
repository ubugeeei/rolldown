//! Versioned C ABI and safe Rust SDK for Rolldown native plugins.
//!
//! This crate is experimental. ABI v1 is checked at runtime, but the exposed
//! hook surface and distribution conventions may change before stabilization.
//!
//! The ABI deliberately exposes only C-layout values. Rust plugins should use
//! [`NativePlugin`] and [`register_plugin!`] instead of implementing the raw
//! entry points themselves.

#![deny(clippy::undocumented_unsafe_blocks, unsafe_op_in_unsafe_fn)]

use std::{ffi::c_void, marker::PhantomData, panic::AssertUnwindSafe};

use bitflags::bitflags;

pub const ABI_VERSION: u32 = 1;
pub const INIT_SYMBOL: &[u8] = b"rolldown_plugin_init\0";

pub const STATUS_OK: u8 = 0;
pub const STATUS_ERROR: u8 = 1;
pub const STATUS_INCOMPATIBLE: u8 = 2;

pub const EXTERNAL_NONE: u8 = 0;
pub const EXTERNAL_FALSE: u8 = 1;
pub const EXTERNAL_TRUE: u8 = 2;
pub const EXTERNAL_ABSOLUTE: u8 = 3;
pub const EXTERNAL_RELATIVE: u8 = 4;

pub const SIDE_EFFECTS_NONE: u8 = 0;
pub const SIDE_EFFECTS_TRUE: u8 = 1;
pub const SIDE_EFFECTS_FALSE: u8 = 2;
pub const SIDE_EFFECTS_NO_TREESHAKE: u8 = 3;

pub const IMPORT_KIND_IMPORT: u8 = 0;
pub const IMPORT_KIND_DYNAMIC_IMPORT: u8 = 1;
pub const IMPORT_KIND_REQUIRE: u8 = 2;
pub const IMPORT_KIND_AT_IMPORT: u8 = 3;
pub const IMPORT_KIND_URL_IMPORT: u8 = 4;
pub const IMPORT_KIND_NEW_URL: u8 = 5;
pub const IMPORT_KIND_HOT_ACCEPT: u8 = 6;

bitflags! {
  #[derive(Debug, Clone, Copy, PartialEq, Eq)]
  pub struct HookUsage: u64 {
    const BUILD_START = 1 << 0;
    const RESOLVE_ID = 1 << 1;
    const LOAD = 1 << 2;
    const TRANSFORM = 1 << 3;
    const BUILD_END = 1 << 4;
  }
}

/// Raw C-layout types. They are public so non-Rust SDKs can be generated and
/// the host loader can share the exact same definitions.
pub mod ffi {
  use super::c_void;

  #[repr(C)]
  #[derive(Debug, Clone, Copy)]
  pub struct Header {
    pub abi_version: u32,
    pub size: usize,
  }

  impl Header {
    pub fn new<T>() -> Self {
      Self { abi_version: super::ABI_VERSION, size: size_of::<T>() }
    }
  }

  impl Default for Header {
    fn default() -> Self {
      Self { abi_version: super::ABI_VERSION, size: 0 }
    }
  }

  #[repr(C)]
  #[derive(Debug, Clone, Copy, Default)]
  pub struct StringView {
    pub data: *const u8,
    pub len: usize,
  }

  impl StringView {
    pub fn new(value: &str) -> Self {
      Self { data: value.as_ptr(), len: value.len() }
    }
  }

  #[repr(C)]
  #[derive(Debug, Clone, Copy, Default)]
  pub struct OptionalStringView {
    pub value: StringView,
    pub present: u8,
    pub reserved: [u8; 7],
  }

  impl OptionalStringView {
    pub fn new(value: Option<&str>) -> Self {
      value.map_or_else(Self::default, |value| Self {
        value: StringView::new(value),
        present: 1,
        reserved: [0; 7],
      })
    }
  }

  #[repr(C)]
  #[derive(Debug, Default)]
  pub struct Buffer {
    pub data: *mut u8,
    pub len: usize,
    /// Opaque host allocation capacity. Plugins must preserve this value.
    pub capacity: usize,
  }

  #[repr(C)]
  #[derive(Debug, Default)]
  pub struct OptionalBuffer {
    pub value: Buffer,
    pub present: u8,
    pub reserved: [u8; 7],
  }

  pub type BufferAllocFn = unsafe extern "C" fn(len: usize, output: *mut Buffer) -> u8;
  pub type AddWatchFileFn = unsafe extern "C" fn(context: *mut c_void, path: StringView) -> u8;

  #[repr(C)]
  #[derive(Debug)]
  pub struct HostApi {
    pub header: Header,
    pub buffer_alloc: Option<BufferAllocFn>,
    pub add_watch_file: Option<AddWatchFileFn>,
  }

  #[repr(C)]
  #[derive(Debug, Clone, Copy)]
  pub struct HookContext {
    pub header: Header,
    pub host: *const HostApi,
    pub handle: *mut c_void,
    pub cwd: StringView,
  }

  #[repr(C)]
  #[derive(Debug)]
  pub struct InitArgs {
    pub header: Header,
    pub host: *const HostApi,
    pub config: StringView,
  }

  #[repr(C)]
  #[derive(Debug)]
  pub struct BuildStartArgs {
    pub header: Header,
    pub context: HookContext,
  }

  #[repr(C)]
  #[derive(Debug)]
  pub struct ResolveIdArgs {
    pub header: Header,
    pub context: HookContext,
    pub specifier: StringView,
    pub importer: OptionalStringView,
    pub kind: u8,
    pub is_entry: u8,
    pub reserved: [u8; 6],
  }

  #[repr(C)]
  #[derive(Debug)]
  pub struct LoadArgs {
    pub header: Header,
    pub context: HookContext,
    pub id: StringView,
  }

  #[repr(C)]
  #[derive(Debug)]
  pub struct TransformArgs {
    pub header: Header,
    pub context: HookContext,
    pub id: StringView,
    pub code: StringView,
    pub module_type: StringView,
  }

  #[repr(C)]
  #[derive(Debug)]
  pub struct BuildEndArgs {
    pub header: Header,
    pub context: HookContext,
    pub error_count: usize,
  }

  #[repr(C)]
  #[derive(Debug, Default)]
  pub struct ResolveIdOutput {
    pub header: Header,
    pub handled: u8,
    pub external: u8,
    pub side_effects: u8,
    pub reserved: [u8; 5],
    pub id: Buffer,
  }

  #[repr(C)]
  #[derive(Debug, Default)]
  pub struct LoadOutput {
    pub header: Header,
    pub handled: u8,
    pub side_effects: u8,
    pub reserved: [u8; 6],
    pub code: Buffer,
    pub map: OptionalBuffer,
    pub module_type: OptionalBuffer,
  }

  #[repr(C)]
  #[derive(Debug, Default)]
  pub struct TransformOutput {
    pub header: Header,
    pub handled: u8,
    pub side_effects: u8,
    pub reserved: [u8; 6],
    pub code: OptionalBuffer,
    pub map: OptionalBuffer,
    pub module_type: OptionalBuffer,
  }

  pub type DeinitFn = unsafe extern "C" fn(state: *mut c_void);

  pub type BuildStartFn =
    unsafe extern "C" fn(state: *mut c_void, args: *const BuildStartArgs, error: *mut Buffer) -> u8;

  pub type ResolveIdFn = unsafe extern "C" fn(
    state: *mut c_void,
    args: *const ResolveIdArgs,
    output: *mut ResolveIdOutput,
    error: *mut Buffer,
  ) -> u8;

  pub type LoadFn = unsafe extern "C" fn(
    state: *mut c_void,
    args: *const LoadArgs,
    output: *mut LoadOutput,
    error: *mut Buffer,
  ) -> u8;

  pub type TransformFn = unsafe extern "C" fn(
    state: *mut c_void,
    args: *const TransformArgs,
    output: *mut TransformOutput,
    error: *mut Buffer,
  ) -> u8;

  pub type BuildEndFn =
    unsafe extern "C" fn(state: *mut c_void, args: *const BuildEndArgs, error: *mut Buffer) -> u8;

  #[repr(C)]
  #[derive(Debug)]
  pub struct PluginDescriptor {
    pub header: Header,
    pub name: StringView,
    pub state: *mut c_void,
    pub hook_usage: u64,
    pub deinit: Option<DeinitFn>,
    pub build_start: Option<BuildStartFn>,
    pub resolve_id: Option<ResolveIdFn>,
    pub load: Option<LoadFn>,
    pub transform: Option<TransformFn>,
    pub build_end: Option<BuildEndFn>,
  }

  impl Default for PluginDescriptor {
    fn default() -> Self {
      Self {
        header: Header::new::<Self>(),
        name: StringView::default(),
        state: std::ptr::null_mut(),
        hook_usage: 0,
        deinit: None,
        build_start: None,
        resolve_id: None,
        load: None,
        transform: None,
        build_end: None,
      }
    }
  }

  pub type InitFn = unsafe extern "C" fn(
    args: *const InitArgs,
    plugin: *mut PluginDescriptor,
    error: *mut Buffer,
  ) -> u8;
}

#[derive(Debug, Clone, Copy)]
pub enum External {
  False,
  True,
  Absolute,
  Relative,
}

#[derive(Debug, Clone, Copy)]
pub enum SideEffects {
  True,
  False,
  NoTreeshake,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportKind {
  Import,
  DynamicImport,
  Require,
  AtImport,
  UrlImport,
  NewUrl,
  HotAccept,
  /// A kind added by a newer host. Plugins should conservatively return
  /// `None` when they do not understand it.
  Unknown(u8),
}

impl From<u8> for ImportKind {
  fn from(value: u8) -> Self {
    match value {
      IMPORT_KIND_IMPORT => Self::Import,
      IMPORT_KIND_DYNAMIC_IMPORT => Self::DynamicImport,
      IMPORT_KIND_REQUIRE => Self::Require,
      IMPORT_KIND_AT_IMPORT => Self::AtImport,
      IMPORT_KIND_URL_IMPORT => Self::UrlImport,
      IMPORT_KIND_NEW_URL => Self::NewUrl,
      IMPORT_KIND_HOT_ACCEPT => Self::HotAccept,
      value => Self::Unknown(value),
    }
  }
}

#[derive(Debug)]
pub struct ResolveIdOutput {
  pub id: String,
  pub external: Option<External>,
  pub side_effects: Option<SideEffects>,
}

impl ResolveIdOutput {
  pub fn new(id: impl Into<String>) -> Self {
    Self { id: id.into(), external: None, side_effects: None }
  }
}

#[derive(Debug)]
pub struct LoadOutput {
  pub code: String,
  pub map: Option<String>,
  pub module_type: Option<String>,
  pub side_effects: Option<SideEffects>,
}

impl LoadOutput {
  pub fn code(code: impl Into<String>) -> Self {
    Self { code: code.into(), map: None, module_type: None, side_effects: None }
  }
}

#[derive(Debug, Default)]
pub struct TransformOutput {
  pub code: Option<String>,
  pub map: Option<String>,
  pub module_type: Option<String>,
  pub side_effects: Option<SideEffects>,
}

impl TransformOutput {
  pub fn code(code: impl Into<String>) -> Self {
    Self { code: Some(code.into()), ..Self::default() }
  }
}

#[derive(Clone, Copy)]
pub struct PluginContext<'a> {
  raw: &'a ffi::HookContext,
  cwd: &'a str,
}

impl std::fmt::Debug for PluginContext<'_> {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("PluginContext").field("cwd", &self.cwd).finish()
  }
}

impl<'a> PluginContext<'a> {
  fn from_validated(raw: &'a ffi::HookContext, cwd: &'a str) -> Self {
    Self { raw, cwd }
  }

  pub fn cwd(&self) -> &'a str {
    self.cwd
  }

  pub fn add_watch_file(&self, path: &str) -> Result<(), String> {
    // SAFETY: The host table and context handle are valid for this hook call.
    unsafe {
      let host = self.raw.host.as_ref().ok_or("missing Rolldown host API")?;
      let callback = host.add_watch_file.ok_or("host does not support addWatchFile")?;
      match callback(self.raw.handle, ffi::StringView::new(path)) {
        STATUS_OK => Ok(()),
        _ => Err("Rolldown rejected the watch path".into()),
      }
    }
  }
}

#[derive(Debug, Clone, Copy)]
pub struct ResolveIdArgs<'a> {
  pub specifier: &'a str,
  pub importer: Option<&'a str>,
  pub kind: ImportKind,
  pub is_entry: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct LoadArgs<'a> {
  pub id: &'a str,
}

#[derive(Debug, Clone, Copy)]
pub struct TransformArgs<'a> {
  pub id: &'a str,
  pub code: &'a str,
  pub module_type: &'a str,
}

/// Safe Rust interface implemented by a native plugin.
pub trait NativePlugin: Send + Sync + 'static {
  const NAME: &'static str;
  const HOOKS: HookUsage;

  fn new(config: &[u8]) -> Result<Self, String>
  where
    Self: Sized;

  fn build_start(&self, _ctx: PluginContext<'_>) -> Result<(), String> {
    Ok(())
  }

  fn resolve_id(
    &self,
    _ctx: PluginContext<'_>,
    _args: ResolveIdArgs<'_>,
  ) -> Result<Option<ResolveIdOutput>, String> {
    Ok(None)
  }

  fn load(
    &self,
    _ctx: PluginContext<'_>,
    _args: LoadArgs<'_>,
  ) -> Result<Option<LoadOutput>, String> {
    Ok(None)
  }

  fn transform(
    &self,
    _ctx: PluginContext<'_>,
    _args: TransformArgs<'_>,
  ) -> Result<Option<TransformOutput>, String> {
    Ok(None)
  }

  fn build_end(&self, _ctx: PluginContext<'_>, _error_count: usize) -> Result<(), String> {
    Ok(())
  }
}

#[macro_export]
macro_rules! register_plugin {
  ($plugin:ty) => {
    /// Initialize the plugin through Rolldown's native ABI.
    ///
    /// # Safety
    ///
    /// The caller must provide ABI-compatible pointers whose storage remains
    /// valid for the duration of this call.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn rolldown_plugin_init(
      args: *const $crate::ffi::InitArgs,
      plugin: *mut $crate::ffi::PluginDescriptor,
      error: *mut $crate::ffi::Buffer,
    ) -> u8 {
      // SAFETY: Rolldown calls the exported symbol with ABI v1 pointers.
      unsafe { $crate::__private::init::<$plugin>(args, plugin, error) }
    }
  };
}

#[doc(hidden)]
pub mod __private {
  use super::{
    ABI_VERSION, AssertUnwindSafe, ImportKind, LoadArgs, NativePlugin, PhantomData, PluginContext,
    ResolveIdArgs, STATUS_ERROR, STATUS_INCOMPATIBLE, STATUS_OK, TransformArgs, c_void,
    encode_external, encode_side_effects, ffi, optional_view_as_str, panic_message, view_as_bytes,
    view_as_str,
  };

  trait HasContext {
    fn header(&self) -> &ffi::Header;
    fn context(&self) -> &ffi::HookContext;
  }

  macro_rules! impl_has_context {
    ($($ty:ty),+ $(,)?) => {
      $(
        impl HasContext for $ty {
          fn header(&self) -> &ffi::Header {
            &self.header
          }

          fn context(&self) -> &ffi::HookContext {
            &self.context
          }
        }
      )+
    };
  }

  impl_has_context!(
    ffi::BuildStartArgs,
    ffi::ResolveIdArgs,
    ffi::LoadArgs,
    ffi::TransformArgs,
    ffi::BuildEndArgs,
  );

  trait HasOutputHeader {
    fn header(&self) -> &ffi::Header;
  }

  macro_rules! impl_has_output_header {
    ($($ty:ty),+ $(,)?) => {
      $(
        impl HasOutputHeader for $ty {
          fn header(&self) -> &ffi::Header {
            &self.header
          }
        }
      )+
    };
  }

  impl_has_output_header!(ffi::ResolveIdOutput, ffi::LoadOutput, ffi::TransformOutput);

  struct State<T> {
    plugin: T,
    _not_send_by_accident: PhantomData<*const ()>,
  }

  // The public trait requires the contained plugin to be Send + Sync. The raw
  // pointer marker prevents auto traits until that contract is checked here.
  // SAFETY: State contains only T, and NativePlugin requires T: Send + Sync.
  unsafe impl<T: NativePlugin> Send for State<T> {}

  // SAFETY: State contains only T, and NativePlugin requires T: Send + Sync.
  unsafe impl<T: NativePlugin> Sync for State<T> {}

  /// Implements the exported native-plugin initializer.
  ///
  /// # Safety
  ///
  /// `args`, `output`, and `error` must follow the ABI v1 pointer, lifetime,
  /// initialization, and aliasing contract documented in `rolldown_plugin.h`.
  pub unsafe fn init<T: NativePlugin>(
    args: *const ffi::InitArgs,
    output: *mut ffi::PluginDescriptor,
    error: *mut ffi::Buffer,
  ) -> u8 {
    // SAFETY: The ABI caller must pass either null or a readable InitArgs.
    let Some(args) = (unsafe { args.as_ref() }) else {
      return STATUS_INCOMPATIBLE;
    };
    if args.header.abi_version != ABI_VERSION || args.header.size < size_of::<ffi::InitArgs>() {
      return STATUS_INCOMPATIBLE;
    }
    // The host initializes this header to advertise how many descriptor bytes
    // are writable. Reading it before constructing T also avoids leaking
    // plugin state when the output slot is incompatible.
    // SAFETY: The ABI caller passes initialized PluginDescriptor storage or
    // null; as_ref rejects the null case before reading the header.
    let Some(output_header) = (unsafe { output.as_ref() }).map(|output| output.header) else {
      return STATUS_INCOMPATIBLE;
    };
    if output_header.abi_version != ABI_VERSION
      || output_header.size < size_of::<ffi::PluginDescriptor>()
    {
      return STATUS_INCOMPATIBLE;
    }
    // SAFETY: InitArgs keeps its host table alive for the duration of init.
    let Some(host) = (unsafe { args.host.as_ref() }) else {
      return STATUS_INCOMPATIBLE;
    };
    if host.header.abi_version != ABI_VERSION
      || host.header.size < size_of::<ffi::HostApi>()
      || host.buffer_alloc.is_none()
    {
      return STATUS_INCOMPATIBLE;
    }
    // SAFETY: The init contract keeps config.data readable for config.len bytes.
    let config = match unsafe { view_as_bytes(args.config) } {
      Ok(config) => config,
      Err(message) => {
        // SAFETY: error is the caller-provided output slot for this init call.
        unsafe { write_error(host, error, &message) };
        return STATUS_ERROR;
      }
    };
    let plugin = match std::panic::catch_unwind(AssertUnwindSafe(|| T::new(config))) {
      Ok(Ok(plugin)) => plugin,
      Ok(Err(message)) => {
        // SAFETY: error is the caller-provided output slot for this init call.
        unsafe { write_error(host, error, &message) };
        return STATUS_ERROR;
      }
      Err(payload) => {
        // SAFETY: error is the caller-provided output slot for this init call.
        unsafe { write_error(host, error, &panic_message(payload.as_ref())) };
        return STATUS_ERROR;
      }
    };
    let state = Box::new(State { plugin, _not_send_by_accident: PhantomData });
    // SAFETY: output was checked non-null above and points to caller-owned,
    // writable PluginDescriptor storage for this call.
    unsafe {
      output.write(ffi::PluginDescriptor {
        header: ffi::Header::new::<ffi::PluginDescriptor>(),
        name: ffi::StringView::new(T::NAME),
        state: Box::into_raw(state).cast(),
        hook_usage: T::HOOKS.bits(),
        deinit: Some(deinit::<T>),
        build_start: Some(build_start::<T>),
        resolve_id: Some(resolve_id::<T>),
        load: Some(load::<T>),
        transform: Some(transform::<T>),
        build_end: Some(build_end::<T>),
      });
    }
    STATUS_OK
  }

  unsafe extern "C" fn deinit<T: NativePlugin>(state: *mut c_void) {
    if !state.is_null() {
      // SAFETY: init created this pointer with Box::into_raw for exactly this
      // State<T>, and the ABI requires deinit to be called at most once.
      let _ = std::panic::catch_unwind(AssertUnwindSafe(|| unsafe {
        drop(Box::from_raw(state.cast::<State<T>>()));
      }));
    }
  }

  unsafe extern "C" fn build_start<T: NativePlugin>(
    state: *mut c_void,
    args: *const ffi::BuildStartArgs,
    error: *mut ffi::Buffer,
  ) -> u8 {
    // SAFETY: This ABI entry point forwards its caller-validated raw pointers;
    // call performs null, version, and size checks before using their fields.
    unsafe { call(state, args, error, |plugin: &T, _args, context| plugin.build_start(context)) }
  }

  unsafe extern "C" fn resolve_id<T: NativePlugin>(
    state: *mut c_void,
    args: *const ffi::ResolveIdArgs,
    output: *mut ffi::ResolveIdOutput,
    error: *mut ffi::Buffer,
  ) -> u8 {
    // SAFETY: This ABI entry point forwards its caller-validated raw pointers;
    // call performs null, version, and size checks before using their fields.
    unsafe {
      call(state, args, error, |plugin: &T, args, context| {
        if args.is_entry > 1 {
          return Err("Rolldown passed an invalid is_entry boolean".into());
        }
        let result = plugin.resolve_id(
          context,
          ResolveIdArgs {
            specifier: view_as_str(args.specifier)?,
            importer: optional_view_as_str(args.importer)?,
            kind: ImportKind::from(args.kind),
            is_entry: args.is_entry != 0,
          },
        )?;
        if let Some(result) = result {
          let output = validate_output(output, "resolveId")?;
          output.handled = 1;
          output.external = encode_external(result.external);
          output.side_effects = encode_side_effects(result.side_effects);
          write_buffer(&args.context, &mut output.id, result.id.as_bytes())?;
        }
        Ok(())
      })
    }
  }

  unsafe extern "C" fn load<T: NativePlugin>(
    state: *mut c_void,
    args: *const ffi::LoadArgs,
    output: *mut ffi::LoadOutput,
    error: *mut ffi::Buffer,
  ) -> u8 {
    // SAFETY: This ABI entry point forwards its caller-validated raw pointers;
    // call performs null, version, and size checks before using their fields.
    unsafe {
      call(state, args, error, |plugin: &T, args, context| {
        let result = plugin.load(context, LoadArgs { id: view_as_str(args.id)? })?;
        if let Some(result) = result {
          let output = validate_output(output, "load")?;
          output.handled = 1;
          output.side_effects = encode_side_effects(result.side_effects);
          write_buffer(&args.context, &mut output.code, result.code.as_bytes())?;
          write_optional_buffer(&args.context, &mut output.map, result.map.as_deref())?;
          write_optional_buffer(
            &args.context,
            &mut output.module_type,
            result.module_type.as_deref(),
          )?;
        }
        Ok(())
      })
    }
  }

  unsafe extern "C" fn transform<T: NativePlugin>(
    state: *mut c_void,
    args: *const ffi::TransformArgs,
    output: *mut ffi::TransformOutput,
    error: *mut ffi::Buffer,
  ) -> u8 {
    // SAFETY: This ABI entry point forwards its caller-validated raw pointers;
    // call performs null, version, and size checks before using their fields.
    unsafe {
      call(state, args, error, |plugin: &T, args, context| {
        let result = plugin.transform(
          context,
          TransformArgs {
            id: view_as_str(args.id)?,
            code: view_as_str(args.code)?,
            module_type: view_as_str(args.module_type)?,
          },
        )?;
        if let Some(result) = result {
          let output = validate_output(output, "transform")?;
          output.handled = 1;
          output.side_effects = encode_side_effects(result.side_effects);
          write_optional_buffer(&args.context, &mut output.code, result.code.as_deref())?;
          write_optional_buffer(&args.context, &mut output.map, result.map.as_deref())?;
          write_optional_buffer(
            &args.context,
            &mut output.module_type,
            result.module_type.as_deref(),
          )?;
        }
        Ok(())
      })
    }
  }

  unsafe extern "C" fn build_end<T: NativePlugin>(
    state: *mut c_void,
    args: *const ffi::BuildEndArgs,
    error: *mut ffi::Buffer,
  ) -> u8 {
    // SAFETY: This ABI entry point forwards its caller-validated raw pointers;
    // call performs null, version, and size checks before using their fields.
    unsafe {
      call(state, args, error, |plugin: &T, args, context| {
        plugin.build_end(context, args.error_count)
      })
    }
  }

  unsafe fn call<T: NativePlugin, A: HasContext>(
    state: *mut c_void,
    args: *const A,
    error: *mut ffi::Buffer,
    callback: impl FnOnce(&T, &A, PluginContext<'_>) -> Result<(), String>,
  ) -> u8 {
    // SAFETY: init created state as State<T>; the host retains it until deinit.
    let state = unsafe { state.cast::<State<T>>().as_ref() };
    // SAFETY: Each hook contract keeps its argument struct readable for the call.
    let args = unsafe { args.as_ref() };
    let (Some(state), Some(args)) = (state, args) else {
      return STATUS_ERROR;
    };
    if args.header().abi_version != ABI_VERSION
      || args.header().size < size_of::<A>()
      || args.context().header.abi_version != ABI_VERSION
      || args.context().header.size < size_of::<ffi::HookContext>()
    {
      return STATUS_INCOMPATIBLE;
    }
    let Some(host) = hook_host(args) else {
      return STATUS_INCOMPATIBLE;
    };
    if host.header.abi_version != ABI_VERSION
      || host.header.size < size_of::<ffi::HostApi>()
      || host.buffer_alloc.is_none()
    {
      return STATUS_INCOMPATIBLE;
    }
    // SAFETY: The validated HookContext keeps cwd readable for this call.
    let Ok(cwd) = (unsafe { view_as_str(args.context().cwd) }) else {
      return STATUS_INCOMPATIBLE;
    };
    let context = PluginContext::from_validated(args.context(), cwd);
    match std::panic::catch_unwind(AssertUnwindSafe(|| callback(&state.plugin, args, context))) {
      Ok(Ok(())) => STATUS_OK,
      Ok(Err(message)) => {
        // SAFETY: error is the caller-provided output slot for this hook call.
        unsafe { write_error(host, error, &message) };
        STATUS_ERROR
      }
      Err(payload) => {
        // SAFETY: error is the caller-provided output slot for this hook call.
        unsafe { write_error(host, error, &panic_message(payload.as_ref())) };
        STATUS_ERROR
      }
    }
  }

  fn hook_host<A: HasContext>(args: &A) -> Option<&ffi::HostApi> {
    // SAFETY: The HookContext is borrowed from the current ABI call, which
    // requires its host table to outlive the call.
    unsafe { args.context().host.as_ref() }
  }

  fn validate_output<'a, T: HasOutputHeader>(
    output: *mut T,
    hook_name: &str,
  ) -> Result<&'a mut T, String> {
    // SAFETY: The hook caller provides writable T storage or null. as_mut does
    // not read fields until the null case has been rejected.
    let output = unsafe { output.as_mut() }.ok_or_else(|| format!("missing {hook_name} output"))?;
    let header = output.header();
    if header.abi_version != ABI_VERSION || header.size < size_of::<T>() {
      return Err(format!("incompatible {hook_name} output"));
    }
    Ok(output)
  }

  fn write_buffer(
    context: &ffi::HookContext,
    output: &mut ffi::Buffer,
    bytes: &[u8],
  ) -> Result<(), String> {
    // SAFETY: HookContext guarantees that host lives through the hook call.
    let host = unsafe { context.host.as_ref() }.ok_or("missing Rolldown host API")?;
    // SAFETY: output is a field in the caller-owned hook output and host is the
    // validated table associated with the same call.
    unsafe { allocate_and_copy(host, output, bytes) }
  }

  fn write_optional_buffer(
    context: &ffi::HookContext,
    output: &mut ffi::OptionalBuffer,
    value: Option<&str>,
  ) -> Result<(), String> {
    if let Some(value) = value {
      write_buffer(context, &mut output.value, value.as_bytes())?;
      output.present = 1;
    }
    Ok(())
  }

  unsafe fn write_error(host: &ffi::HostApi, output: *mut ffi::Buffer, message: &str) {
    // SAFETY: The ABI caller owns output and keeps it writable during the call.
    if let Some(output) = unsafe { output.as_mut() } {
      // SAFETY: output is now a valid Buffer reference and host was validated.
      let _ = unsafe { allocate_and_copy(host, output, message.as_bytes()) };
    }
  }

  unsafe fn allocate_and_copy(
    host: &ffi::HostApi,
    output: &mut ffi::Buffer,
    bytes: &[u8],
  ) -> Result<(), String> {
    let allocate = host.buffer_alloc.ok_or("host does not provide a buffer allocator")?;
    // SAFETY: allocate is supplied by the validated host table, and output is
    // writable for the current hook call.
    if unsafe { allocate(bytes.len(), output) } != STATUS_OK {
      return Err("Rolldown could not allocate a plugin output buffer".into());
    }
    if !bytes.is_empty() {
      if output.data.is_null() || output.len != bytes.len() || output.capacity < bytes.len() {
        return Err("Rolldown returned an invalid plugin output buffer".into());
      }
      // SAFETY: The host returned a non-null allocation of exactly bytes.len;
      // source and destination belong to distinct allocations.
      unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), output.data, bytes.len()) };
    }
    Ok(())
  }
}

fn encode_external(value: Option<External>) -> u8 {
  match value {
    None => EXTERNAL_NONE,
    Some(External::False) => EXTERNAL_FALSE,
    Some(External::True) => EXTERNAL_TRUE,
    Some(External::Absolute) => EXTERNAL_ABSOLUTE,
    Some(External::Relative) => EXTERNAL_RELATIVE,
  }
}

fn encode_side_effects(value: Option<SideEffects>) -> u8 {
  match value {
    None => SIDE_EFFECTS_NONE,
    Some(SideEffects::True) => SIDE_EFFECTS_TRUE,
    Some(SideEffects::False) => SIDE_EFFECTS_FALSE,
    Some(SideEffects::NoTreeshake) => SIDE_EFFECTS_NO_TREESHAKE,
  }
}

fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
  if let Some(message) = payload.downcast_ref::<&str>() {
    format!("native plugin panicked: {message}")
  } else if let Some(message) = payload.downcast_ref::<String>() {
    format!("native plugin panicked: {message}")
  } else {
    "native plugin panicked with a non-string payload".into()
  }
}

unsafe fn view_as_bytes<'a>(view: ffi::StringView) -> Result<&'a [u8], String> {
  if view.len == 0 {
    Ok(&[])
  } else if view.data.is_null() {
    Err("Rolldown passed a null string pointer with a non-zero length".into())
  } else {
    // SAFETY: The ABI caller guarantees data is readable for len bytes. The
    // null/non-zero-length case was rejected above.
    Ok(unsafe { std::slice::from_raw_parts(view.data, view.len) })
  }
}

unsafe fn view_as_str<'a>(view: ffi::StringView) -> Result<&'a str, String> {
  // SAFETY: This function carries the same borrowed-view contract as
  // view_as_bytes and immediately validates UTF-8.
  std::str::from_utf8(unsafe { view_as_bytes(view) }?)
    .map_err(|_| "Rolldown passed non-UTF-8 text to a native plugin".into())
}

unsafe fn optional_view_as_str<'a>(
  view: ffi::OptionalStringView,
) -> Result<Option<&'a str>, String> {
  match view.present {
    0 => Ok(None),
    // SAFETY: present=1 activates the StringView under the same call lifetime.
    1 => unsafe { view_as_str(view.value) }.map(Some),
    _ => Err("Rolldown passed an invalid optional string discriminant".into()),
  }
}

#[cfg(test)]
mod tests {
  use std::{
    fmt::Write,
    sync::{
      Mutex,
      atomic::{AtomicUsize, Ordering},
    },
  };

  use super::*;

  static WATCH_CALLS: AtomicUsize = AtomicUsize::new(0);
  static DROPS: AtomicUsize = AtomicUsize::new(0);
  static ECHO_TEST_LOCK: Mutex<()> = Mutex::new(());

  unsafe extern "C" fn test_buffer_alloc(len: usize, output: *mut ffi::Buffer) -> u8 {
    // SAFETY: SDK tests pass a writable Buffer output slot to this callback.
    let Some(output) = (unsafe { output.as_mut() }) else {
      return STATUS_ERROR;
    };
    if len == 0 {
      *output = ffi::Buffer::default();
      return STATUS_OK;
    }
    let mut bytes = Vec::<u8>::with_capacity(len);
    *output = ffi::Buffer { data: bytes.as_mut_ptr(), len, capacity: bytes.capacity() };
    std::mem::forget(bytes);
    STATUS_OK
  }

  unsafe extern "C" fn test_add_watch_file(_context: *mut c_void, path: ffi::StringView) -> u8 {
    // SAFETY: The SDK keeps this borrowed StringView alive for the callback.
    if unsafe { view_as_str(path) }.as_deref() != Ok("watched.js") {
      return STATUS_ERROR;
    }
    WATCH_CALLS.fetch_add(1, Ordering::Relaxed);
    STATUS_OK
  }

  static TEST_HOST: ffi::HostApi = ffi::HostApi {
    header: ffi::Header { abi_version: ABI_VERSION, size: size_of::<ffi::HostApi>() },
    buffer_alloc: Some(test_buffer_alloc),
    add_watch_file: Some(test_add_watch_file),
  };

  unsafe extern "C" fn undersized_buffer_alloc(len: usize, output: *mut ffi::Buffer) -> u8 {
    // SAFETY: This test callback receives a writable Buffer output slot.
    let Some(output) = (unsafe { output.as_mut() }) else {
      return STATUS_ERROR;
    };
    *output = ffi::Buffer { data: std::ptr::NonNull::<u8>::dangling().as_ptr(), len, capacity: 0 };
    STATUS_OK
  }

  static UNDERSIZED_BUFFER_HOST: ffi::HostApi = ffi::HostApi {
    header: ffi::Header { abi_version: ABI_VERSION, size: size_of::<ffi::HostApi>() },
    buffer_alloc: Some(undersized_buffer_alloc),
    add_watch_file: Some(test_add_watch_file),
  };

  fn hook_context() -> ffi::HookContext {
    ffi::HookContext {
      header: ffi::Header::new::<ffi::HookContext>(),
      host: &raw const TEST_HOST,
      handle: std::ptr::null_mut(),
      cwd: ffi::StringView::new("/workspace"),
    }
  }

  fn take_test_buffer(buffer: &mut ffi::Buffer) -> Vec<u8> {
    if buffer.data.is_null() {
      assert_eq!(buffer.len, 0);
      assert_eq!(buffer.capacity, 0);
      return Vec::new();
    }
    // SAFETY: Test buffers come only from test_buffer_alloc and are consumed
    // exactly once with the capacity returned by that allocator.
    let bytes = unsafe { Vec::from_raw_parts(buffer.data, buffer.len, buffer.capacity) };
    *buffer = ffi::Buffer::default();
    bytes
  }

  fn take_test_string(buffer: &mut ffi::Buffer) -> String {
    String::from_utf8(take_test_buffer(buffer)).unwrap()
  }

  fn init_plugin<T: NativePlugin>(config: &str) -> ffi::PluginDescriptor {
    let args = ffi::InitArgs {
      header: ffi::Header::new::<ffi::InitArgs>(),
      host: &raw const TEST_HOST,
      config: ffi::StringView::new(config),
    };
    let mut descriptor = ffi::PluginDescriptor::default();
    let mut error = ffi::Buffer::default();
    // SAFETY: All pointers target initialized stack values that outlive init.
    let status =
      unsafe { __private::init::<T>(&raw const args, &raw mut descriptor, &raw mut error) };
    assert_eq!(status, STATUS_OK);
    assert_eq!(take_test_buffer(&mut error), Vec::<u8>::new());
    descriptor
  }

  fn invoke_transform(
    descriptor: &ffi::PluginDescriptor,
    args: &ffi::TransformArgs,
    output: &mut ffi::TransformOutput,
    error: &mut ffi::Buffer,
  ) -> u8 {
    // SAFETY: init_plugin produced the function/state pair, and every stack
    // value remains alive until the synchronous hook returns.
    unsafe {
      descriptor.transform.unwrap()(
        descriptor.state,
        std::ptr::from_ref(args),
        std::ptr::from_mut(output),
        std::ptr::from_mut(error),
      )
    }
  }

  fn invoke_transform_parts(
    transform: ffi::TransformFn,
    state: usize,
    args: &ffi::TransformArgs,
    output: &mut ffi::TransformOutput,
    error: &mut ffi::Buffer,
  ) -> u8 {
    // SAFETY: The function/state pair comes from one live descriptor. The hook
    // is synchronous, so every borrowed stack value outlives the call.
    unsafe {
      transform(
        state as *mut c_void,
        std::ptr::from_ref(args),
        std::ptr::from_mut(output),
        std::ptr::from_mut(error),
      )
    }
  }

  fn invoke_init<T: NativePlugin>(
    args: Option<&ffi::InitArgs>,
    descriptor: &mut ffi::PluginDescriptor,
    error: &mut ffi::Buffer,
  ) -> u8 {
    // SAFETY: Non-null pointers refer to initialized stack values for the
    // synchronous call; None intentionally exercises the null-input path.
    unsafe {
      __private::init::<T>(
        args.map_or(std::ptr::null(), std::ptr::from_ref),
        std::ptr::from_mut(descriptor),
        std::ptr::from_mut(error),
      )
    }
  }

  fn deinit_plugin(descriptor: &mut ffi::PluginDescriptor) {
    // SAFETY: init_plugin created this state. Taking deinit ensures this helper
    // is the sole owner that releases it.
    unsafe { descriptor.deinit.take().unwrap()(descriptor.state) };
    descriptor.state = std::ptr::null_mut();
  }

  fn descriptor_name(descriptor: &ffi::PluginDescriptor) -> &str {
    // SAFETY: Test plugin names are static strings and outlive the descriptor.
    unsafe { view_as_str(descriptor.name) }.unwrap()
  }

  struct EchoPlugin {
    prefix: String,
  }

  impl Drop for EchoPlugin {
    fn drop(&mut self) {
      DROPS.fetch_add(1, Ordering::Relaxed);
    }
  }

  impl NativePlugin for EchoPlugin {
    const NAME: &'static str = "test:echo";
    const HOOKS: HookUsage = HookUsage::TRANSFORM;

    fn new(config: &[u8]) -> Result<Self, String> {
      Ok(Self {
        prefix: std::str::from_utf8(config).map_err(|error| error.to_string())?.to_string(),
      })
    }

    fn transform(
      &self,
      context: PluginContext<'_>,
      args: TransformArgs<'_>,
    ) -> Result<Option<TransformOutput>, String> {
      assert_eq!(context.cwd(), "/workspace");
      assert_eq!(args.id, "entry.js");
      assert_eq!(args.module_type, "js");
      context.add_watch_file("watched.js")?;
      Ok(Some(TransformOutput {
        code: Some(format!("{}{}", self.prefix, args.code)),
        map: Some(r#"{"version":3,"sources":[],"names":[],"mappings":""}"#.into()),
        module_type: Some("js".into()),
        side_effects: Some(SideEffects::False),
      }))
    }
  }

  struct FailingInitPlugin;

  impl NativePlugin for FailingInitPlugin {
    const NAME: &'static str = "test:failing-init";
    const HOOKS: HookUsage = HookUsage::empty();

    fn new(_config: &[u8]) -> Result<Self, String> {
      Err("invalid configuration".into())
    }
  }

  struct PanickingPlugin;

  impl NativePlugin for PanickingPlugin {
    const NAME: &'static str = "test:panic";
    const HOOKS: HookUsage = HookUsage::TRANSFORM;

    fn new(_config: &[u8]) -> Result<Self, String> {
      Ok(Self)
    }

    fn transform(
      &self,
      _context: PluginContext<'_>,
      _args: TransformArgs<'_>,
    ) -> Result<Option<TransformOutput>, String> {
      panic!("intentional test panic")
    }
  }

  #[test]
  fn ffi_types_are_size_prefixed() {
    assert_eq!(ffi::Header::new::<ffi::HostApi>().size, size_of::<ffi::HostApi>());
    assert_eq!(ffi::PluginDescriptor::default().header.size, size_of::<ffi::PluginDescriptor>());
  }

  #[test]
  fn hook_bits_are_stable() {
    assert_eq!(HookUsage::TRANSFORM.bits(), 1 << 3);
    assert_eq!(HookUsage::all().bits(), 0b1_1111);
  }

  #[test]
  fn import_kinds_are_stable_and_forward_compatible() {
    assert_eq!(ImportKind::from(IMPORT_KIND_IMPORT), ImportKind::Import);
    assert_eq!(ImportKind::from(IMPORT_KIND_DYNAMIC_IMPORT), ImportKind::DynamicImport);
    assert_eq!(ImportKind::from(IMPORT_KIND_HOT_ACCEPT), ImportKind::HotAccept);
    assert_eq!(ImportKind::from(u8::MAX), ImportKind::Unknown(u8::MAX));
  }

  #[test]
  fn c_header_layout_matches_rust_ffi_types() {
    let mut source = String::from(
      "#include <stddef.h>\n#include \"rolldown_plugin.h\"\n\
       #if defined(__cplusplus) || defined(_MSC_VER)\n\
       #define ASSERT_LAYOUT(c, m) static_assert(c, m)\n\
       #else\n#define ASSERT_LAYOUT(c, m) _Static_assert(c, m)\n#endif\n",
    );
    macro_rules! assert_layout {
      ($rust:ty, $c:literal, $($field:ident),+ $(,)?) => {{
        writeln!(
          source,
          "ASSERT_LAYOUT(sizeof({}) == {}, \"{} size\");",
          $c,
          size_of::<$rust>(),
          $c,
        )
        .unwrap();
        $(
          writeln!(
            source,
            "ASSERT_LAYOUT(offsetof({}, {}) == {}, \"{}.{} offset\");",
            $c,
            stringify!($field),
            std::mem::offset_of!($rust, $field),
            $c,
            stringify!($field),
          )
          .unwrap();
        )+
      }};
    }
    assert_layout!(ffi::Header, "RolldownHeader", abi_version, size);
    assert_layout!(ffi::StringView, "RolldownStringView", data, len);
    assert_layout!(ffi::OptionalStringView, "RolldownOptionalStringView", value, present, reserved);
    assert_layout!(ffi::Buffer, "RolldownBuffer", data, len, capacity);
    assert_layout!(ffi::OptionalBuffer, "RolldownOptionalBuffer", value, present, reserved);
    assert_layout!(ffi::HostApi, "RolldownHostApi", header, buffer_alloc, add_watch_file);
    assert_layout!(ffi::HookContext, "RolldownHookContext", header, host, handle, cwd);
    assert_layout!(ffi::InitArgs, "RolldownInitArgs", header, host, config);
    assert_layout!(ffi::BuildStartArgs, "RolldownBuildStartArgs", header, context);
    assert_layout!(
      ffi::ResolveIdArgs,
      "RolldownResolveIdArgs",
      header,
      context,
      specifier,
      importer,
      kind,
      is_entry,
      reserved
    );
    assert_layout!(ffi::LoadArgs, "RolldownLoadArgs", header, context, id);
    assert_layout!(
      ffi::TransformArgs,
      "RolldownTransformArgs",
      header,
      context,
      id,
      code,
      module_type
    );
    assert_layout!(ffi::BuildEndArgs, "RolldownBuildEndArgs", header, context, error_count);
    assert_layout!(
      ffi::ResolveIdOutput,
      "RolldownResolveIdOutput",
      header,
      handled,
      external,
      side_effects,
      reserved,
      id
    );
    assert_layout!(
      ffi::LoadOutput,
      "RolldownLoadOutput",
      header,
      handled,
      side_effects,
      reserved,
      code,
      map,
      module_type
    );
    assert_layout!(
      ffi::TransformOutput,
      "RolldownTransformOutput",
      header,
      handled,
      side_effects,
      reserved,
      code,
      map,
      module_type
    );
    assert_layout!(
      ffi::PluginDescriptor,
      "RolldownPluginDescriptor",
      header,
      name,
      state,
      hook_usage,
      deinit,
      build_start,
      resolve_id,
      load,
      transform,
      build_end
    );

    let temp_dir =
      std::env::temp_dir().join(format!("rolldown-plugin-abi-layout-{}", std::process::id()));
    std::fs::create_dir_all(&temp_dir).unwrap();
    let target = if cfg!(all(target_arch = "aarch64", target_os = "macos")) {
      "aarch64-apple-darwin"
    } else if cfg!(all(target_arch = "x86_64", target_os = "macos")) {
      "x86_64-apple-darwin"
    } else if cfg!(all(target_arch = "x86_64", target_os = "windows")) {
      "x86_64-pc-windows-msvc"
    } else if cfg!(all(target_arch = "aarch64", target_os = "linux")) {
      "aarch64-unknown-linux-gnu"
    } else {
      "x86_64-unknown-linux-gnu"
    };
    for (cplusplus, extension) in [(false, "c"), (true, "cc")] {
      let source_path = temp_dir.join(format!("layout.{extension}"));
      let object_path = temp_dir.join(format!("layout-{extension}.o"));
      std::fs::write(&source_path, &source).unwrap();
      let mut build = cc::Build::new();
      let compiler = build
        .cpp(cplusplus)
        .cargo_metadata(false)
        .target(target)
        .host(target)
        .opt_level(0)
        .debug(false)
        .get_compiler();
      let mut command = compiler.to_command();
      if compiler.is_like_msvc() {
        command
          .arg("/nologo")
          .arg(if cplusplus { "/std:c++17" } else { "/std:c11" })
          .arg("/c")
          .arg(&source_path)
          // spellchecker:ignore-next-line
          .arg(format!("/Fo{}", object_path.display()))
          .arg(format!("/I{}", env!("CARGO_MANIFEST_DIR")));
      } else {
        command
          .arg(if cplusplus { "-std=c++17" } else { "-std=c11" })
          .arg("-c")
          .arg(&source_path)
          .arg("-o")
          .arg(&object_path)
          .arg("-I")
          .arg(env!("CARGO_MANIFEST_DIR"));
      }
      let output = command.output().unwrap();
      assert!(
        output.status.success(),
        "{} header layout check failed:\n{}",
        if cplusplus { "C++" } else { "C" },
        String::from_utf8_lossy(&output.stderr)
      );
    }
    std::fs::remove_dir_all(temp_dir).unwrap();
  }

  #[test]
  fn sdk_marshals_transform_outputs_and_deinitializes_once() {
    let _guard = ECHO_TEST_LOCK.lock().unwrap();
    WATCH_CALLS.store(0, Ordering::Relaxed);
    DROPS.store(0, Ordering::Relaxed);
    let mut descriptor = init_plugin::<EchoPlugin>("prefix:");
    assert_eq!(descriptor.hook_usage, HookUsage::TRANSFORM.bits());
    assert_eq!(descriptor_name(&descriptor), "test:echo");

    let args = ffi::TransformArgs {
      header: ffi::Header::new::<ffi::TransformArgs>(),
      context: hook_context(),
      id: ffi::StringView::new("entry.js"),
      code: ffi::StringView::new("input"),
      module_type: ffi::StringView::new("js"),
    };
    let mut output = ffi::TransformOutput {
      header: ffi::Header::new::<ffi::TransformOutput>(),
      ..ffi::TransformOutput::default()
    };
    let mut error = ffi::Buffer::default();
    let status = invoke_transform(&descriptor, &args, &mut output, &mut error);
    assert_eq!(status, STATUS_OK);
    assert_eq!(output.handled, 1);
    assert_eq!(output.side_effects, SIDE_EFFECTS_FALSE);
    assert_eq!(output.code.present, 1);
    assert_eq!(take_test_string(&mut output.code.value), "prefix:input");
    assert_eq!(output.map.present, 1);
    assert!(take_test_string(&mut output.map.value).contains("\"version\":3"));
    assert_eq!(take_test_string(&mut output.module_type.value), "js");
    assert_eq!(take_test_buffer(&mut error), Vec::<u8>::new());
    assert_eq!(WATCH_CALLS.load(Ordering::Relaxed), 1);

    deinit_plugin(&mut descriptor);
    assert_eq!(DROPS.load(Ordering::Relaxed), 1);
  }

  #[test]
  fn sdk_converts_init_errors_to_host_owned_buffers() {
    let args = ffi::InitArgs {
      header: ffi::Header::new::<ffi::InitArgs>(),
      host: &raw const TEST_HOST,
      config: ffi::StringView::default(),
    };
    let mut descriptor = ffi::PluginDescriptor::default();
    let mut error = ffi::Buffer::default();
    assert_eq!(
      invoke_init::<FailingInitPlugin>(Some(&args), &mut descriptor, &mut error),
      STATUS_ERROR
    );
    assert_eq!(take_test_string(&mut error), "invalid configuration");
    assert!(descriptor.state.is_null());
  }

  #[test]
  fn sdk_catches_hook_panics() {
    let mut descriptor = init_plugin::<PanickingPlugin>("");
    let args = ffi::TransformArgs {
      header: ffi::Header::new::<ffi::TransformArgs>(),
      context: hook_context(),
      id: ffi::StringView::new("entry.js"),
      code: ffi::StringView::new("input"),
      module_type: ffi::StringView::new("js"),
    };
    let mut output = ffi::TransformOutput {
      header: ffi::Header::new::<ffi::TransformOutput>(),
      ..ffi::TransformOutput::default()
    };
    let mut error = ffi::Buffer::default();
    assert_eq!(invoke_transform(&descriptor, &args, &mut output, &mut error), STATUS_ERROR);
    assert_eq!(take_test_string(&mut error), "native plugin panicked: intentional test panic");
    deinit_plugin(&mut descriptor);
  }

  #[test]
  fn sdk_rejects_undersized_host_allocations_before_copying() {
    let _guard = ECHO_TEST_LOCK.lock().unwrap();
    let mut descriptor = init_plugin::<EchoPlugin>("prefix:");
    let args = ffi::TransformArgs {
      header: ffi::Header::new::<ffi::TransformArgs>(),
      context: ffi::HookContext {
        header: ffi::Header::new::<ffi::HookContext>(),
        host: &raw const UNDERSIZED_BUFFER_HOST,
        handle: std::ptr::null_mut(),
        cwd: ffi::StringView::new("/workspace"),
      },
      id: ffi::StringView::new("entry.js"),
      code: ffi::StringView::new("input"),
      module_type: ffi::StringView::new("js"),
    };
    let mut output = ffi::TransformOutput {
      header: ffi::Header::new::<ffi::TransformOutput>(),
      ..ffi::TransformOutput::default()
    };
    let mut error = ffi::Buffer::default();

    assert_eq!(invoke_transform(&descriptor, &args, &mut output, &mut error), STATUS_ERROR);
    assert_eq!(output.code.value.capacity, 0);
    assert_eq!(error.capacity, 0);

    // Both pointers are deliberate non-allocations from the hostile test host.
    // Clear them without constructing a Vec or dereferencing either pointer.
    output.code.value = ffi::Buffer::default();
    error = ffi::Buffer::default();
    assert!(output.code.value.data.is_null());
    assert!(error.data.is_null());
    deinit_plugin(&mut descriptor);
  }

  #[test]
  fn sdk_supports_overlapping_hook_calls() {
    let _guard = ECHO_TEST_LOCK.lock().unwrap();
    WATCH_CALLS.store(0, Ordering::Relaxed);
    let mut descriptor = init_plugin::<EchoPlugin>("parallel:");
    let transform = descriptor.transform.unwrap();
    let state = descriptor.state as usize;
    let threads = (0..8)
      .map(|_| {
        std::thread::spawn(move || {
          let args = ffi::TransformArgs {
            header: ffi::Header::new::<ffi::TransformArgs>(),
            context: hook_context(),
            id: ffi::StringView::new("entry.js"),
            code: ffi::StringView::new("input"),
            module_type: ffi::StringView::new("js"),
          };
          let mut output = ffi::TransformOutput {
            header: ffi::Header::new::<ffi::TransformOutput>(),
            ..ffi::TransformOutput::default()
          };
          let mut error = ffi::Buffer::default();
          assert_eq!(
            invoke_transform_parts(transform, state, &args, &mut output, &mut error),
            STATUS_OK
          );
          assert_eq!(take_test_string(&mut output.code.value), "parallel:input");
          let _ = take_test_buffer(&mut output.map.value);
          let _ = take_test_buffer(&mut output.module_type.value);
        })
      })
      .collect::<Vec<_>>();
    for thread in threads {
      thread.join().unwrap();
    }
    assert_eq!(WATCH_CALLS.load(Ordering::Relaxed), 8);
    deinit_plugin(&mut descriptor);
  }

  #[test]
  fn sdk_rejects_incompatible_and_malformed_inputs() {
    let mut descriptor = ffi::PluginDescriptor::default();
    let mut error = ffi::Buffer::default();
    assert_eq!(invoke_init::<EchoPlugin>(None, &mut descriptor, &mut error), STATUS_INCOMPATIBLE);

    let valid_args = ffi::InitArgs {
      header: ffi::Header::new::<ffi::InitArgs>(),
      host: &raw const TEST_HOST,
      config: ffi::StringView::default(),
    };
    descriptor.header.size = 0;
    assert_eq!(
      invoke_init::<EchoPlugin>(Some(&valid_args), &mut descriptor, &mut error),
      STATUS_INCOMPATIBLE
    );
    assert!(descriptor.state.is_null());
    descriptor = ffi::PluginDescriptor::default();

    let args = ffi::InitArgs {
      header: ffi::Header::new::<ffi::InitArgs>(),
      host: &raw const TEST_HOST,
      config: ffi::StringView { data: std::ptr::null(), len: 1 },
    };
    assert_eq!(invoke_init::<EchoPlugin>(Some(&args), &mut descriptor, &mut error), STATUS_ERROR);
    assert!(take_test_string(&mut error).contains("null string pointer"));
  }
}
