import { nativePlugin } from 'rolldown/experimental';
import { describe, expect, test } from 'vitest';

describe('nativePlugin', () => {
  test('accepts an extensionless file URL and serializes config once', () => {
    let reads = 0;
    const config = {
      get target() {
        reads++;
        return 'browser';
      },
    };

    const plugin = nativePlugin({
      library: new URL('file:///plugins/example'),
      config,
      enforce: 'pre',
    });

    expect(plugin).toMatchObject({
      name: 'builtin:native-plugin',
      enforce: 'pre',
      _options: {
        library: 'file:///plugins/example',
        config: '{"target":"browser"}',
      },
    });
    expect(reads).toBe(1);
  });

  test('passes an absolute path without rewriting it in JavaScript', () => {
    const plugin = nativePlugin({ library: '/plugins/example.so' });
    expect(plugin._options).toEqual({
      library: '/plugins/example.so',
      config: undefined,
    });
  });

  test('rejects invalid library locations', () => {
    expect(() => nativePlugin({ library: '' })).toThrow(/library path or URL/);
    expect(() => nativePlugin({ library: new URL('https://example.com/plugin') })).toThrow(
      /file: protocol/,
    );
    expect(() => nativePlugin({ library: 42 as unknown as string })).toThrow(
      /absolute path or file URL/,
    );
  });

  test('rejects non-serializable config', () => {
    expect(() => nativePlugin({ library: '/plugins/example', config: 1n })).toThrow(TypeError);

    const cyclic: { self?: unknown } = {};
    cyclic.self = cyclic;

    expect(() => nativePlugin({ library: '/plugins/example', config: cyclic })).toThrow(TypeError);
  });
});
