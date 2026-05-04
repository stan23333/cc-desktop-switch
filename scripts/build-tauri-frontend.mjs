import { cp, mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const root = dirname(dirname(fileURLToPath(import.meta.url)));
const frontendDir = join(root, 'frontend');
const distDir = join(root, 'dist');

await rm(distDir, { force: true, recursive: true });
await mkdir(distDir, { recursive: true });
await cp(frontendDir, distDir, {
  recursive: true,
  filter: (path) => !path.endsWith('.DS_Store'),
});

const sourceHtml = await readFile(join(frontendDir, 'index.html'), 'utf8');
const tauriHtml = sourceHtml.replace(
  /<script\s+src=["']js\/api\.js["']><\/script>/,
  '<script src="js/tauri-api.js"></script>',
);

if (tauriHtml === sourceHtml) {
  throw new Error('Failed to replace original js/api.js with Tauri API bridge');
}

await writeFile(join(distDir, 'index.html'), tauriHtml);
await rm(join(distDir, 'js', 'api.js'), { force: true });
await cp(join(root, 'src', 'originalApiBridge.js'), join(distDir, 'js', 'tauri-api.js'));
