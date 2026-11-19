import assert from 'node:assert/strict';

const messages = [];
const originalLog = console.log;
console.log = (...args) => messages.push(args.join(' '));

try {
  await import('./dist/main.native.js');
} finally {
  console.log = originalLog;
}

assert.deepEqual(messages, ['Hello directly from a Rust plugin']);
