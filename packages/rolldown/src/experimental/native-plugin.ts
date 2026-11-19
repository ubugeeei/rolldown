import { BuiltinPlugin } from '../builtin-plugin/utils';

/**
 * Options for an experimental native plugin loaded through Rolldown's versioned C ABI.
 *
 * Native plugins are trusted in-process code. Only load libraries you trust.
 *
 * @experimental
 */
export interface NativePluginOptions {
  /**
   * Absolute path or file URL to the shared library. When it has no extension,
   * Rolldown adds the platform's native library prefix and suffix.
   */
  library: string | URL;
  /** JSON-serializable configuration passed to the plugin once at initialization. */
  config?: unknown;
  /** Run before or after plugins without an `enforce` value. */
  enforce?: 'pre' | 'post';
}

function normalizeLibrary(library: string | URL): string {
  if (typeof library === 'string') {
    if (library.length === 0) {
      throw new TypeError('nativePlugin requires a library path or URL');
    }
    return library;
  }

  if (!(library instanceof URL)) {
    throw new TypeError('nativePlugin library must be an absolute path or file URL');
  }
  if (library.protocol !== 'file:') {
    throw new TypeError('nativePlugin library URL must use the file: protocol');
  }

  return library.toString();
}

function serializeConfig(config: unknown): string | undefined {
  if (config === undefined) {
    return undefined;
  }

  const serialized = JSON.stringify(config);
  if (serialized === undefined) {
    throw new TypeError('nativePlugin config must be JSON-serializable');
  }

  return serialized;
}

/**
 * Load a native plugin directly into Rolldown without a per-hook JavaScript or
 * Node-API round trip.
 *
 * This API is unavailable in the WASI/browser build.
 *
 * @experimental
 */
export function nativePlugin(options: NativePluginOptions): BuiltinPlugin {
  const plugin = new BuiltinPlugin('builtin:native-plugin', {
    library: normalizeLibrary(options.library),
    config: serializeConfig(options.config),
  });

  plugin.enforce = options.enforce;
  return plugin;
}
