# Native plugin

This example loads a Rust `cdylib` directly through Rolldown's experimental,
versioned C ABI. Each `transform` call stays in native code and does not cross
through JavaScript or Node-API.

The example deliberately exercises every ABI v1 hook: lifecycle validation in
`buildStart`/`buildEnd`, a virtual module through `resolveId` and `load`, and an
entry-module rewrite plus watch dependency through `transform`.

Build the shared library first:

```sh
cargo build --manifest-path plugin/Cargo.toml --release
```

Then, from this directory, run Rolldown:

```sh
pnpm rolldown -c
node dist/main.native.js
```

The example package uses `rolldown.devBinary` with an extensionless Cargo output
path. The test task links that working package into `node_modules`, so
`plugin.mjs` exercises the same package-name resolution that consumers use.
Rolldown adds the platform's conventional prefix and `.dll`, `.dylib`, or `.so`
suffix for the development path.

A published plugin instead maps target triples to optional artifact packages in
`rolldown.artifacts`. Rolldown selects and loads the package's prebuilt binary in
Rust without importing plugin JavaScript.

`rolldown_plugin_native_abi` also ships `rolldown_plugin.h` for plugins
implemented in C or any language that can export a C ABI.

## Language conformance examples

The `foreign` directory contains real C, C++, Go, and Zig shared libraries.
They all implement the same ABI from `rolldown_plugin.h`; CMake writes their
platform-specific binaries to `artifacts`, while `conformance.mjs` loads only
extensionless file URLs.

With CMake, Go, and Zig installed, run the complete language suite from the
repository root:

```sh
just test-native-plugin
```

CI runs this suite on Linux, macOS, and Windows. The Rust example remains part
of the default Node test suite because it does not require additional language
toolchains.

The Go example deliberately returns from Go before its C bridge calls any host
function. Go's `c-shared` runtime is process-lifetime and has no supported
unload operation, so the conformance runner isolates that plugin in a child
process and exits only after the build has closed. Account for that lifecycle
constraint before choosing Go for an in-process production plugin.

## Boundary benchmark

From the repository root, `just bench-native-plugin` builds release binaries
and compares equivalent native and JavaScript hooks over 5,000 generated entry
modules. The harness alternates execution order, performs three warmups, reports
the median of eleven runs, and requires byte-equivalent generated output.

This synthetic workload measures the integration boundary; it is not evidence
that every native plugin or framework compiler will be faster. Override the
workload with `NATIVE_PLUGIN_BENCH_FILES`, `NATIVE_PLUGIN_BENCH_WARMUPS`, and
`NATIVE_PLUGIN_BENCH_RUNS`.
