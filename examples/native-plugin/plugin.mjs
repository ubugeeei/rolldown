import { nativePlugin } from 'rolldown/experimental';

export function exampleNativePlugin(config) {
  return nativePlugin({
    // The loader adds the platform prefix and suffix (`lib` where applicable,
    // then `.so`, `.dylib`, or `.dll`). A published package would point this
    // URL at its installed platform artifact instead of a Cargo target.
    library: new URL('./plugin/target/release/native_plugin_example', import.meta.url),
    config,
  });
}
