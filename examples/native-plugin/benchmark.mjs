import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { performance } from 'node:perf_hooks';

import { rolldown } from 'rolldown';

import { exampleNativePlugin } from './plugin.mjs';

const VIRTUAL_SPECIFIER = 'virtual:native-message';
const VIRTUAL_ID = '\0native-plugin:message';

function positiveInteger(name, fallback) {
  const value = Number.parseInt(process.env[name] ?? String(fallback), 10);
  if (!Number.isSafeInteger(value) || value <= 0) {
    throw new TypeError(`${name} must be a positive integer`);
  }
  return value;
}

const fileCount = positiveInteger('NATIVE_PLUGIN_BENCH_FILES', 5000);
const warmupCount = positiveInteger('NATIVE_PLUGIN_BENCH_WARMUPS', 3);
const runCount = positiveInteger('NATIVE_PLUGIN_BENCH_RUNS', 11);
const message = 'native plugin benchmark';

function javascriptPlugin() {
  let started = false;
  let resolved = 0;
  let loaded = 0;
  let transformed = 0;

  return {
    name: 'example:javascript-plugin',
    buildStart() {
      assert.equal(started, false, 'buildStart was called more than once');
      started = true;
    },
    resolveId(specifier) {
      if (specifier !== VIRTUAL_SPECIFIER) return null;
      resolved++;
      return { id: VIRTUAL_ID, external: false, moduleSideEffects: false };
    },
    load(id) {
      if (id !== VIRTUAL_ID) return null;
      loaded++;
      return {
        code: `export default ${JSON.stringify(message)};`,
        moduleType: 'js',
        moduleSideEffects: false,
      };
    },
    transform(code, id) {
      if (!id.endsWith('.native.js')) return null;
      transformed++;
      this.addWatchFile(id);
      return code.replace('__NATIVE_LOG__', 'console.log');
    },
    buildEnd(error) {
      assert.equal(error, undefined);
      assert.equal(started, true);
      assert.ok(resolved > 0);
      assert.ok(loaded > 0);
      assert.ok(transformed > 0);
    },
  };
}

function digest(output) {
  const hash = createHash('sha256');
  const sortedOutput = [...output].sort(compareFileName);

  for (const item of sortedOutput) {
    hash.update(item.fileName);
    hash.update('\0');
    hash.update(item.type === 'chunk' ? item.code : item.source);
    hash.update('\0');
  }
  return hash.digest('hex');
}

function compareFileName(left, right) {
  return left.fileName.localeCompare(right.fileName);
}

async function bundle(input, plugin) {
  const start = performance.now();
  const build = await rolldown({ input, plugins: [plugin], treeshake: false });
  try {
    const { output } = await build.generate({ format: 'esm' });
    return { duration: performance.now() - start, digest: digest(output) };
  } finally {
    await build.close();
  }
}

function median(values) {
  const sorted = [...values].sort((left, right) => left - right);
  return sorted[Math.floor(sorted.length / 2)];
}

function createPlugin(implementation) {
  if (implementation === 'native') {
    return exampleNativePlugin({ message });
  }
  return javascriptPlugin();
}

function implementationOrder(index) {
  if (index % 2 === 0) {
    return ['native', 'javascript'];
  }
  return ['javascript', 'native'];
}

async function createFixture(directory) {
  const entries = await Promise.all(
    Array.from({ length: fileCount }, async (_, index) => {
      const path = join(directory, `entry-${index}.native.js`);
      const source = [
        `import message from ${JSON.stringify(VIRTUAL_SPECIFIER)};`,
        `__NATIVE_LOG__(message, ${index});`,
        '',
      ].join('\n');

      await writeFile(path, source);
      return [`entry-${index}`, path];
    }),
  );

  return Object.fromEntries(entries);
}

function printReport(samples, outputSha256) {
  const nativeMedian = median(samples.native);
  const javascriptMedian = median(samples.javascript);
  const report = {
    workload: { files: fileCount, warmups: warmupCount, runs: runCount },
    medianMs: {
      native: Number(nativeMedian.toFixed(2)),
      javascript: Number(javascriptMedian.toFixed(2)),
    },
    javascriptOverNative: Number((javascriptMedian / nativeMedian).toFixed(3)),
    outputSha256,
  };

  console.log(JSON.stringify(report, null, 2));
}

const fixtureDirectory = await mkdtemp(join(tmpdir(), 'rolldown-native-plugin-benchmark-'));

try {
  const input = await createFixture(fixtureDirectory);
  const samples = { native: [], javascript: [] };
  let expectedDigest;

  for (let index = -warmupCount; index < runCount; index++) {
    for (const implementation of implementationOrder(index)) {
      const result = await bundle(input, createPlugin(implementation));
      expectedDigest ??= result.digest;

      assert.equal(result.digest, expectedDigest, `${implementation} output differs`);
      if (index >= 0) {
        samples[implementation].push(result.duration);
      }
    }
  }

  printReport(samples, expectedDigest);
} finally {
  await rm(fixtureDirectory, { recursive: true, force: true });
}
