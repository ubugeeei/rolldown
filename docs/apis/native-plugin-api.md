# Native Plugin API <Badge type="warning" text="Experimental" />

::: warning Experimental
This API is available only from `rolldown/experimental`. Its JavaScript API,
ABI surface, and binary distribution conventions may change before
stabilization. Pin the Rolldown and `rolldown_plugin_native_abi` versions when
evaluating it in production.
:::

Native plugins run performance-critical `resolveId`, `load`, and `transform`
hooks directly inside Rolldown through a versioned C ABI. A handled hook does
not cross JavaScript or Node-API.

It is not available in the WASI or browser build.

## Loading a plugin

```ts
import { defineConfig } from 'rolldown';
import { nativePlugin } from 'rolldown/experimental';

export default defineConfig({
  input: './src/main.js',
  plugins: [
    nativePlugin({
      // Rolldown adds the platform's conventional library prefix and suffix.
      library: new URL('./native/plugin', import.meta.url),
      config: { message: 'Hello from native code' },
      enforce: 'pre',
    }),
  ],
});
```

`library` must be an absolute path or file URL. If it has no extension,
Rolldown adds the current platform's dynamic-library prefix and suffix.
`config` must be JSON-serializable and is serialized once during option
normalization. `enforce` uses the same ordering as JavaScript plugins.

Native libraries are trusted in-process code with the same permissions as
Rolldown. Never load an untrusted plugin.

Rolldown canonicalizes the path and requires a regular file before loading.
Unix loads use immediate relocation and local symbol visibility. Windows
dependency lookup is restricted to the library directory and Windows' safe
default directories, excluding the current working directory. These checks
reduce accidental misloading and DLL search-path attacks; they do not sandbox
the library or protect against a malicious plugin.

## Writing a native plugin

Rust plugins should use the safe `rolldown_plugin_native_abi` SDK. Other
languages implement the same interface from `rolldown_plugin.h`. These are the
actual conformance plugins built and loaded in the repository test suite:

::: code-group

<<< ../../examples/native-plugin/plugin/src/lib.rs [Rust]

<<< ../../examples/native-plugin/foreign/c/plugin.c [C]

<<< ../../examples/native-plugin/foreign/cpp/plugin.cpp [C++]

<<< ../../examples/native-plugin/foreign/go/main.go [Go]

<<< ../../examples/native-plugin/foreign/zig/plugin.zig [Zig]

:::

The Go example uses a small C bridge for function-pointer callbacks. Go writes
into C-owned temporary memory and returns before the bridge calls the Rolldown
host API; this avoids a Go → C → host callback while the Go runtime is active.
CMake builds the C, C++, Go, and Zig libraries with the platform's conventional
prefix and suffix. All five language examples are loaded through extensionless
file URLs.

::: warning Go runtime lifetime
Go `-buildmode=c-shared` embeds a process-lifetime runtime that cannot be
unloaded safely. It may also keep a Node process from completing implicit event
loop shutdown. The conformance suite therefore runs the Go plugin in an
isolated process and terminates that process after `build.close()`. Production
integrations must account for this runtime property; prefer Rust, C++, C, or Zig
when clean in-process unload and implicit Node shutdown are required.
:::

See the [complete native plugin example][native-plugin-example] for a
configuration and buildable plugin.

[native-plugin-example]: https://github.com/rolldown/rolldown/tree/main/examples/native-plugin

## ABI v1

ABI v1 supports `buildStart`, `resolveId`, `load`, `transform`, and `buildEnd`.
Hook calls may overlap on multiple threads. Rust plugins must be `Send + Sync`;
plugins written against the C header must provide equivalent synchronization.

Inputs are borrowed for one call and must not be retained. Outputs are allocated
with the host allocator and owned by Rolldown after return. The SDK catches Rust
panics and reports them as plugin errors; exceptions must never cross the C ABI.

The published crate contains `rolldown_plugin.h` for C and other languages with
C interoperability.

## Distribution

A native binary is platform-specific even though its configuration is not.
Production packages should keep platform selection inside the plugin package:

- publish one artifact package for each supported OS and architecture;
- list exact-version artifact packages as optional dependencies of a small
  wrapper package;
- avoid install-time download or compilation scripts;
- expose a plugin factory that passes a fixed, extensionless file URL to
  `nativePlugin`.

Consumers should not need a `process.platform` switch. Standard package-manager
integrity checks and lockfiles remain part of the trust boundary.

## Performance

Hook inputs are borrowed for the duration of one call. Configuration is
serialized once, and output buffers are allocated by Rolldown and transferred
back without an additional copy. Rolldown does not put a global allocation
registry or mutex on the hook hot path.

The repository includes `just bench-native-plugin`, an end-to-end synthetic
comparison against equivalent JavaScript hooks. It validates identical output,
alternates execution order, warms up both paths, and reports medians. Treat it
as a boundary-cost benchmark, not as a promise that arbitrary native code is
faster; framework integrations should publish their own representative
end-to-end measurements.
