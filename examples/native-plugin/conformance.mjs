import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

import { rolldown } from 'rolldown';
import { nativePlugin } from 'rolldown/experimental';

const languages = ['c', 'cpp', 'go', 'zig'];
const input = fileURLToPath(new URL('./src/conformance.foreign.js', import.meta.url));

async function testPlugin(language) {
  const library = new URL(`./artifacts/native_plugin_${language}`, import.meta.url);
  const build = await rolldown({
    input,
    plugins: [nativePlugin({ library })],
  });

  try {
    const { output } = await build.generate({ format: 'esm' });
    const chunks = output.filter((item) => item.type === 'chunk');

    assert.equal(chunks.length, 1, `${language}: expected exactly one output chunk`);
    assert.match(chunks[0].code, new RegExp(`['"]${language}['"]`));
  } finally {
    await build.close();
  }

  console.log(`native ${language} plugin: ok`);
}

const childLanguage = process.argv[2];

if (childLanguage) {
  assert.ok(languages.includes(childLanguage), `unknown language: ${childLanguage}`);
  await testPlugin(childLanguage);

  // A c-shared Go runtime owns process-lifetime threads and has no supported
  // unload operation. The isolated conformance process must terminate itself
  // after Rolldown has completed and closed the build.
  process.exit(0);
}

for (const language of languages) {
  if (language === 'go') {
    const result = spawnSync(process.execPath, [fileURLToPath(import.meta.url), language], {
      stdio: 'inherit',
    });

    assert.equal(result.signal, null, 'native go plugin process was terminated');
    assert.equal(result.status, 0, 'native go plugin process failed');
  } else {
    await testPlugin(language);
  }
}
