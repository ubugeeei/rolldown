import { nativePlugin } from 'rolldown/experimental';
import { describe, expect, test } from 'vitest';

describe('nativePlugin', () => {
  test('passes config to the native binding without serializing in JavaScript', () => {
    let reads = 0;
    const config = {
      get target() {
        reads++;
        return 'browser';
      },
    };

    const plugin = nativePlugin(new URL('file:///plugins/example'), {
      config,
      enforce: 'pre',
    });

    const options = plugin._options as { source: string; config?: unknown };
    expect(plugin.name).toBe('builtin:native-plugin');
    expect(plugin.enforce).toBe('pre');
    expect(options.source).toBe('file:///plugins/example');
    expect(options.config).toBe(config);
    expect(reads).toBe(0);
  });

  test('accepts an npm package name without resolving it in JavaScript', () => {
    const plugin = nativePlugin('@example/rolldown-plugin-native');

    expect(plugin).toMatchObject({
      name: 'builtin:native-plugin',
      _options: {
        source: '@example/rolldown-plugin-native',
      },
    });
  });

  test('passes an absolute path without rewriting it in JavaScript', () => {
    const plugin = nativePlugin('/plugins/example.so');
    expect(plugin._options).toEqual({
      source: '/plugins/example.so',
      config: undefined,
    });
  });

  test('rejects invalid file locations', () => {
    expect(() => nativePlugin('')).toThrow(/package name, absolute path, or file URL/);
    expect(() => nativePlugin(new URL('https://example.com/plugin'))).toThrow(/file: protocol/);
    expect(() => nativePlugin(42 as unknown as string)).toThrow(/absolute path or file URL/);
  });
});
