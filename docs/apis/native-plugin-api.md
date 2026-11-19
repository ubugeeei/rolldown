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
    nativePlugin('@example/rolldown-plugin-native', {
      // Plugin-defined JSON passed once to NativePlugin::new.
      config: { message: 'Hello from native code' },
      enforce: 'pre',
    }),
  ],
});
```

For an npm package, Rolldown reads the package metadata and selects the artifact
for its compiled target. It does not import JavaScript from the plugin package.
An absolute path or local file URL is also accepted for local development. If
that path has no extension, Rolldown adds the current platform's dynamic-library
prefix and suffix.
`config` is optional plugin-defined JSON data. The native binding converts it
once and passes it to the plugin during initialization without interpreting it.
`enforce` uses the same ordering as JavaScript plugins.

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

See `examples/native-plugin` for the complete configuration and buildable
plugin.

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

A native binary is platform-specific even though its configuration is not. The
main npm package contains metadata rather than a JavaScript wrapper:

```json
{
  "name": "@example/rolldown-plugin-native",
  "rolldown": {
    "abi": 1,
    "artifacts": {
      "aarch64-apple-darwin": "@example/rolldown-plugin-native-darwin-arm64",
      "x86_64-unknown-linux-gnu": "@example/rolldown-plugin-native-linux-x64-gnu",
      "x86_64-pc-windows-msvc": "@example/rolldown-plugin-native-win32-x64-msvc"
    }
  },
  "optionalDependencies": {
    "@example/rolldown-plugin-native-darwin-arm64": "1.0.0",
    "@example/rolldown-plugin-native-linux-x64-gnu": "1.0.0",
    "@example/rolldown-plugin-native-win32-x64-msvc": "1.0.0"
  }
}
```

Each artifact package contains one prebuilt library and identifies it in its
own metadata:

```json
{
  "name": "@example/rolldown-plugin-native-darwin-arm64",
  "os": ["darwin"],
  "cpu": ["arm64"],
  "rolldown": {
    "abi": 1,
    "binary": "plugin.dylib"
  }
}
```

Plugin publishers should:

- publish one artifact package for each supported OS and architecture;
- list exact-version artifact packages as optional dependencies of a small
  metadata package;
- avoid install-time download or compilation scripts;
- use `rolldown.devBinary` for an extensionless local build during development.

Consumers do not write a `node_modules` path, choose a file extension, or use a
`process.platform` switch. Rolldown resolves the package and selects the target
artifact in Rust. Standard package-manager integrity checks and lockfiles
remain part of the trust boundary.

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
