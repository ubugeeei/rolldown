# Rolldown native plugin ABI

This crate contains the versioned C ABI and safe Rust SDK for loading native
plugins directly into Rolldown. The ABI is experimental while the higher-level
API is being validated, but every binary boundary is explicitly versioned.

See `rolldown_plugin.h` for the language-neutral interface and
`examples/native-plugin` in the Rolldown repository for a complete Rust plugin.

```rust
use rolldown_plugin_native_abi::{
  HookUsage, NativePlugin, PluginContext, TransformArgs, TransformOutput, register_plugin,
};

struct ExamplePlugin;

impl NativePlugin for ExamplePlugin {
  const NAME: &'static str = "example:native";
  const HOOKS: HookUsage = HookUsage::TRANSFORM;

  fn new(_config: &[u8]) -> Result<Self, String> {
    Ok(Self)
  }

  fn transform(
    &self,
    _ctx: PluginContext<'_>,
    args: TransformArgs<'_>,
  ) -> Result<Option<TransformOutput>, String> {
    Ok(Some(TransformOutput::code(args.code.replace("__DEV__", "false"))))
  }
}

register_plugin!(ExamplePlugin);
```

Plugins are called concurrently and must be `Send + Sync`. Inputs are borrowed
only for the current call. Returned buffers use Rolldown's allocator, and the
SDK prevents Rust panics from unwinding across the C boundary.

Native plugins are trusted in-process code, not a sandbox. Only load binaries
from trusted packages and fixed local paths.
