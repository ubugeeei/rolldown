use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use rolldown_plugin_native_abi::{
  External, HookUsage, LoadArgs, LoadOutput, NativePlugin, PluginContext, ResolveIdArgs,
  ResolveIdOutput, SideEffects, TransformArgs, TransformOutput, register_plugin,
};
use serde::Deserialize;

#[derive(Deserialize)]
struct Config {
  message: String,
}

struct ExamplePlugin {
  message: String,
  started: AtomicBool,
  resolved: AtomicUsize,
  loaded: AtomicUsize,
  transformed: AtomicUsize,
}

impl NativePlugin for ExamplePlugin {
  const NAME: &'static str = "example:native-plugin";
  const HOOKS: HookUsage = HookUsage::all();

  fn new(config: &[u8]) -> Result<Self, String> {
    let config: Config = serde_json::from_slice(config).map_err(|error| error.to_string())?;
    Ok(Self {
      message: config.message,
      started: AtomicBool::new(false),
      resolved: AtomicUsize::new(0),
      loaded: AtomicUsize::new(0),
      transformed: AtomicUsize::new(0),
    })
  }

  fn build_start(&self, _ctx: PluginContext<'_>) -> Result<(), String> {
    if self.started.swap(true, Ordering::Relaxed) {
      return Err("buildStart was called more than once".into());
    }
    Ok(())
  }

  fn resolve_id(
    &self,
    _ctx: PluginContext<'_>,
    args: ResolveIdArgs<'_>,
  ) -> Result<Option<ResolveIdOutput>, String> {
    if args.specifier != "virtual:native-message" {
      return Ok(None);
    }
    self.resolved.fetch_add(1, Ordering::Relaxed);
    Ok(Some(ResolveIdOutput {
      id: "\0native-plugin:message".into(),
      external: Some(External::False),
      side_effects: Some(SideEffects::False),
    }))
  }

  fn load(
    &self,
    _ctx: PluginContext<'_>,
    args: LoadArgs<'_>,
  ) -> Result<Option<LoadOutput>, String> {
    if args.id != "\0native-plugin:message" {
      return Ok(None);
    }
    self.loaded.fetch_add(1, Ordering::Relaxed);
    let message = serde_json::to_string(&self.message).map_err(|error| error.to_string())?;
    Ok(Some(LoadOutput {
      code: format!("export default {message};"),
      map: None,
      module_type: Some("js".into()),
      side_effects: Some(SideEffects::False),
    }))
  }

  fn transform(
    &self,
    ctx: PluginContext<'_>,
    args: TransformArgs<'_>,
  ) -> Result<Option<TransformOutput>, String> {
    if !args.id.ends_with(".native.js") {
      return Ok(None);
    }

    // The load/transform contexts retain this as a module-level invalidation
    // dependency, so edits invalidate the module in watch mode.
    ctx.add_watch_file(args.id)?;
    self.transformed.fetch_add(1, Ordering::Relaxed);
    let code = args.code.replace("__NATIVE_LOG__", "console.log");
    Ok(Some(TransformOutput::code(code)))
  }

  fn build_end(&self, _ctx: PluginContext<'_>, error_count: usize) -> Result<(), String> {
    if error_count != 0 {
      return Err(format!("build finished with {error_count} error(s)"));
    }
    let build_started = self.started.load(Ordering::Relaxed);
    let resolved_module = self.resolved.load(Ordering::Relaxed) > 0;
    let loaded_module = self.loaded.load(Ordering::Relaxed) > 0;
    let transformed_module = self.transformed.load(Ordering::Relaxed) > 0;

    if !(build_started && resolved_module && loaded_module && transformed_module) {
      return Err("not every native ABI v1 hook was exercised".into());
    }
    Ok(())
  }
}

register_plugin!(ExamplePlugin);
