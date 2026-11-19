import { defineConfig } from 'rolldown';
import { exampleNativePlugin } from './plugin.mjs';

export default defineConfig({
  input: './src/main.native.js',
  plugins: [exampleNativePlugin({ message: 'Hello directly from a Rust plugin' })],
  output: {
    dir: './dist',
  },
});
