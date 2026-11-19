# Native plugin ABI — Design & Principles

## Summary

The native plugin ABI lets performance-critical compilers, resolvers, and
transforms run directly inside Rolldown without a per-hook JavaScript or
Node-API round trip. It is a language-neutral, versioned C ABI rather than a
Rust ABI, and is exposed only through `rolldown/experimental` while the hook
surface and packaging model are validated. See [implementation.md](./implementation.md)
for the concrete data flow and [ubugeeei/rolldown#1](https://github.com/ubugeeei/rolldown/issues/1)
for the RFC.

## Motivation

The compiler boundary in a modern framework build is changing. React has an
active Rust port of React Compiler, Solid ships an Oxc-based native compiler,
Astro has accepted a Rust compiler as its future official implementation, and
the Vue and Svelte ecosystems are developing Rust-native compiler toolchains.
Parcel 3 demonstrates the same direction at the bundler layer: its Rust core
can load plugins written in Rust, Go, Swift, or C through a native ABI.

Rolldown can already use these compilers through JavaScript plugins and
Node-API bindings, but that path re-enters JavaScript between two native
components:

```text
Rolldown (Rust) -> Node-API -> JavaScript plugin -> Node-API -> compiler (Rust)
```

That boundary is not free, and it is crossed once for every handled module.
The cost includes converting hook arguments and results into JavaScript
values, transferring string and buffer ownership, scheduling the call through
the JavaScript runtime, and coordinating native work through JavaScript's
execution model. A faster framework compiler does not remove those costs; as
the compiler itself gets faster, fixed interop overhead becomes a larger share
of each hook.

There is also an architectural cost. Rolldown already owns a parallel native
module graph, while native compilers can usually process independent modules
in parallel. Requiring JavaScript between them prevents the two systems from
sharing a direct, thread-safe call path and forces each integration to design
its own Node-API bridge and JavaScript wrapper.

The native plugin ABI adds the missing direct path:

```text
Rolldown (Rust) -> versioned C ABI -> native plugin
```

This is not an argument against JavaScript plugins. The Rollup-compatible API
remains the best default for portability, iteration speed, and the npm
ecosystem. The ABI is a deliberately narrower option for compilers, resolvers,
and transforms that are already native, run on many modules, and can justify
shipping platform-specific binaries. It lets those integrations avoid an
otherwise unnecessary JavaScript round trip without coupling them to Rust's
unstable ABI or to Rolldown's internal crates.

The performance case must remain evidence-based. The design removes specific
copies, conversions, and runtime transitions; it does not promise that every
native plugin is faster. End-to-end benchmarks with real framework compilers
are required before stabilization.

## Design principles

1. **C is the binary boundary.** Rust has no stable ABI. Passing
   `rolldown_plugin::Plugin`, futures, `String`, or any other Rust-layout value
   across a shared-library boundary would couple plugins to a compiler and
   crate build. C-layout integers, pointers, and size-prefixed structs also
   permit Go, Swift, C, and C++ SDKs.
2. **Append instead of breaking.** Every table, argument, descriptor, and
   output begins with an ABI/version header. New fields append to a struct.
   Existing field changes require an ABI version bump; a changed hook signature
   should normally be added as a new function pointer first.
3. **The host owns crossing allocations.** A plugin obtains result buffers
   from `HostApi::buffer_alloc`. Rolldown takes ownership immediately after the
   call. This prevents allocator and Windows CRT mismatches.
4. **Borrow for one call.** Input strings and context handles become invalid
   when a hook returns. Retaining them would also retain module-scoped state
   that is not meaningful in later calls.
5. **Preserve native parallelism.** Rolldown does not serialize calls for a
   plugin. Plugin state must be safe for overlapping calls from different
   threads. The Rust SDK enforces `Send + Sync`.
6. **Do not unwind across FFI.** The Rust SDK catches plugin panics at every
   exported entry point and converts them into hook errors. Other language SDKs
   need the equivalent exception guard.
7. **Start with the hot path.** ABI v1 contains lifecycle plus `resolveId`,
   `load`, and `transform`. Output hooks, async context resolution, and an AST
   ABI should follow real integrations rather than speculative surface area.
8. **Treat libraries as trusted code.** This is not a sandbox. A loaded plugin
   has Rolldown's filesystem, process, and memory permissions. The API accepts
   only an explicit absolute path and documents this trust boundary.

## Why not Node-API?

Node-API remains a useful distribution interface and is supported by the
normal JavaScript plugin API. But a Rust framework compiler called from a JS
plugin follows Rust → Node-API → JavaScript → Node-API → Rust for every module.
Hook filters reduce calls that do no work; they cannot remove the crossings for
files the native compiler actually transforms.

## Why not a Rust plugin dylib?

An in-tree Rust plugin can implement `rolldown_plugin::Plugin` because Rolldown
and the plugin are compiled together. An independently distributed dylib cannot
safely use that trait: trait-object layout, async future types, dependencies,
features, and the Rust compiler version can all differ. The SDK may be Rust,
but its exported boundary stays C.

## Experimental boundary

The JavaScript factory is exported only from `rolldown/experimental`. The ABI
still has strict version and size checks so experimental iteration fails safely
rather than loading incompatible binaries. Stabilization additionally requires
real framework integrations, benchmark evidence, and a cross-platform npm
artifact convention.

## Related

- [implementation.md](./implementation.md) — loader, SDK, hook, and unload flow
- [Parcel native plugins](https://v3.parceljs.org/plugin-system/native-plugins/) — prior art
