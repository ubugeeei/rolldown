import { mkdir, rm, symlink } from 'node:fs/promises';
import { dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const packageRoot = fileURLToPath(new URL('./', import.meta.url));
const packageLink = fileURLToPath(
  new URL('./node_modules/@rolldown/native-plugin-example', import.meta.url),
);

await rm(packageLink, { force: true, recursive: true });
await mkdir(dirname(packageLink), { recursive: true });
await symlink(packageRoot, packageLink, 'junction');
