import { access, cp, mkdir, rm } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { buildAppBundle } from './build-app-bundle.mjs';
import { buildStyleBundle } from './build-style-bundle.mjs';

const root = dirname(dirname(fileURLToPath(import.meta.url)));
const frontendDir = join(root, 'frontend');
const distDir = join(root, 'dist');

await buildAppBundle();
await buildStyleBundle();

await rm(distDir, { force: true, recursive: true });
await mkdir(distDir, { recursive: true });
await cp(frontendDir, distDir, {
  recursive: true,
  filter: (path) => !path.endsWith('.DS_Store'),
});

await access(join(distDir, 'js', 'tauri-api.js'));
