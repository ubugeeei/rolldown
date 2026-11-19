# Native plugin ABI — Implementation

> The rationale and evolution rules live in [design.md](./design.md).

## Components

- `crates/rolldown_plugin_native_abi` owns ABI v1, the canonical C header,
  safe Rust input/output wrappers, `NativePlugin`, and `register_plugin!`.
- `crates/rolldown_plugin_native` opens a dynamic library, validates and owns
  its descriptor, and adapts ABI hooks to `rolldown_plugin::Plugin`.
- `crates/rolldown_binding` recognizes `builtin:native-plugin` during option
  normalization. The builtin representation is an internal transport; users
  construct it with `nativePlugin()` from `rolldown/experimental`.
- `examples/native-plugin` contains a complete Rust `cdylib`, real C, C++, Go,
  and Zig conformance plugins, and their JS configurations.

## Initialization flow

1. The JS factory passes a package name, absolute path, or file URL through the
   binding object. napi-rs converts the JSON-compatible `config` to
   `serde_json::Value`; Rust serializes it once before plugin initialization.
2. For a package name, the loader resolves the package from the build cwd,
   validates `rolldown.abi`, selects `rolldown.artifacts[TARGET]`, resolves the
   artifact package, and reads `rolldown.binary`. This happens in Rust without
   importing plugin JavaScript. Direct extensionless paths receive the target
   platform's standard dynamic-library prefix and suffix.
3. The binding calls `ExternalNativePlugin::load` before bundling starts.
4. The loader opens the library and resolves the one required symbol,
   `rolldown_plugin_init`.
5. Initialization receives the static host function table, borrowed config
   JSON, and a descriptor whose initialized header advertises the writable
   capacity. It validates that header before constructing plugin state, then
   writes a `PluginDescriptor` or an error allocated by the host.
6. The loader validates the ABI version, full v1 descriptor size, hook bits and
   pointers, plugin name UTF-8, and the state/deinit invariant.
7. The descriptor is wrapped by the existing `Pluginable` machinery. Hook
   usage selects only the enabled ABI adapters, exactly as for in-tree plugins.

## Hook flow

For each call the adapter creates stack-owned C-layout arguments. The context
handle points to a stack `HostContext` enum that remembers whether the call is a
normal, load, or transform context. `add_watch_file` dispatches through that
enum so load/transform dependencies retain Rolldown's module-level invalidation
semantics rather than only entering the global watch set.

The plugin returns status plus a size-prefixed output. Strings, source text,
sourcemap JSON, module types, and error messages use host-allocated buffers.
The adapter takes each allocation exactly once, validates UTF-8 and result
discriminants, parses sourcemaps, and converts the result to the normal internal
hook output.

Inputs remain borrowed and outputs transfer ownership without another copy.
There is deliberately no process-global allocation registry or lock on the
hook hot path. A plugin must return the exact pointer and opaque capacity from
the host allocator and fully initialize `len` bytes. Fabricating pointers is
outside the guarantees of this trusted in-process boundary.

## Lifetime and unload invariant

`ExternalNativePlugin` owns the descriptor before the `libloading::Library`
handle. Its explicit `Drop` calls `deinit` exactly once while the handle is
still loaded, clears the state, and only then lets the library unload. The
plugin driver holds the adapter until no hook future can refer to it, so unload
cannot race an active call.

The unsafe `Send`/`Sync` implementation is the C-side equivalent of the Rust
SDK's `NativePlugin: Send + Sync` bound. A hand-written foreign plugin is
responsible for honoring the documented concurrency contract.

Go's `c-shared` runtime has process lifetime and cannot be unloaded safely. Its
conformance plugin therefore runs in an isolated process. The C bridge also
waits for Go to return before invoking the host allocator, avoiding nested
Go → C → host callbacks on platforms where the Go runtime cannot preserve every
host ABI invariant across that re-entry.

## Platform behavior

Desktop Node builds load the current platform's `.dll`, `.dylib`, or `.so`.
WASI and browser builds retain the type surface but reject the plugin during
option normalization with an unsupported-target error. Arbitrary host dynamic
libraries are outside the WASI capability model.

Before loading, a package source is resolved from the build cwd using Node
package resolution. Metadata paths must stay within their package. A direct
path is converted from an absolute path or local file URL and given a platform
prefix/suffix when extensionless. The result is canonicalized and checked as a
regular file. Unix uses `RTLD_NOW | RTLD_LOCAL`. Windows uses
`LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_DEFAULT_DIRS`, avoiding
current-directory dependency lookup.

## Verification

- ABI unit tests compile the canonical header as both C and C++ and compare
  every struct size and field offset with Rust.
- SDK tests cover incompatible inputs, panic conversion, host-owned buffers,
  watch callbacks, concurrent calls, and exactly-once teardown.
- Loader tests cover URL/path policy, platform naming, descriptor validation,
  discriminants, allocation failure, and corrupted ownership fields.
- `just test-native-plugin` builds a real release `cdylib` and exercises every
  ABI v1 hook through the JavaScript API.
- The same command builds C, C++, Go, and Zig shared libraries, loads each via
  an extensionless file URL, and verifies its output. CI runs this suite on
  Linux, macOS, and Windows.
- `just bench-native-plugin` compares equivalent native and JavaScript hooks
  with release binaries, alternating run order and requiring equal output.

## Related

- [design.md](./design.md) — rationale and rejected alternatives
- [ubugeeei/rolldown#1](https://github.com/ubugeeei/rolldown/issues/1) — RFC
