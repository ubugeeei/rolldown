import { BuiltinPlugin } from '../builtin-plugin/utils';

export type NativePluginConfig =
  | null
  | boolean
  | number
  | string
  | readonly NativePluginConfig[]
  | { readonly [key: string]: NativePluginConfig };

/**
 * Options for an experimental native plugin loaded through Rolldown's versioned C ABI.
 *
 * Native plugins are trusted in-process code. Only load libraries you trust.
 *
 * @experimental
 */
export interface NativePluginOptions {
  /** JSON-serializable configuration passed to the plugin once at initialization. */
  config?: NativePluginConfig;
  /** Run before or after plugins without an `enforce` value. */
  enforce?: 'pre' | 'post';
}

function normalizeSource(source: string | URL): string {
  if (typeof source === 'string') {
    if (source.length === 0) {
      throw new TypeError('nativePlugin requires a package name, absolute path, or file URL');
    }
    return source;
  }

  if (!(source instanceof URL)) {
    throw new TypeError('nativePlugin source must be an absolute path or file URL');
  }
  if (source.protocol !== 'file:') {
    throw new TypeError('nativePlugin source URL must use the file: protocol');
  }

  return source.toString();
}

/**
 * Load a native plugin directly into Rolldown without a per-hook JavaScript or
 * Node-API round trip.
 *
 * This API is unavailable in the WASI/browser build.
 *
 * @experimental
 */
export function nativePlugin(
  source: string | URL,
  options: NativePluginOptions = {},
): BuiltinPlugin {
  const plugin = new BuiltinPlugin('builtin:native-plugin', {
    source: normalizeSource(source),
    config: options.config,
  });

  plugin.enforce = options.enforce;
  return plugin;
}
