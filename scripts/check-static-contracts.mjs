import { readFile } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = dirname(dirname(fileURLToPath(import.meta.url)));

async function text(path) {
  return readFile(join(root, path), 'utf8');
}

function assert(condition, message) {
  if (!condition) {
    throw new Error(message);
  }
}

function hasBridgeMethod(source, method) {
  const escaped = method.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  const patterns = [
    new RegExp(`\\b${escaped}\\s*,`),
    new RegExp(`\\basync\\s+${escaped}\\s*\\(`),
    new RegExp(`\\b${escaped}\\s*:\\s*(?:async\\s*)?function\\b`),
    new RegExp(`\\b${escaped}\\s*:\\s*(?:async\\s*)?\\(`),
  ];
  return patterns.some((pattern) => pattern.test(source));
}

const [
  indexHtml,
  appJs,
  bridgeJs,
  buildScript,
  releaseWorkflow,
  windowsBuild,
  tauriConfig,
] = await Promise.all([
  text('frontend/index.html'),
  text('frontend/js/app.js'),
  text('frontend/js/tauri-api.js'),
  text('scripts/build-tauri-frontend.mjs'),
  text('.github/workflows/release.yml'),
  text('windows/build.bat'),
  text('src-tauri/tauri.conf.json'),
]);

assert(
  indexHtml.includes('<script src="js/tauri-api.js"></script>'),
  'frontend/index.html must load the Tauri API bridge directly',
);
assert(!indexHtml.includes('js/api.js'), 'frontend/index.html must not load the retired Python API wrapper');
assert(
  !buildScript.includes('src/originalApiBridge.js') && !buildScript.includes('js/api.js'),
  'Tauri frontend build must not depend on the retired bridge replacement path',
);

const calledMethods = new Set(
  [...appJs.matchAll(/\bCCApi\.([A-Za-z_$][\w$]*)\s*\(/g)].map((match) => match[1]),
);
const missingMethods = [...calledMethods].filter((method) => !hasBridgeMethod(bridgeJs, method));
assert(
  missingMethods.length === 0,
  `frontend/js/tauri-api.js is missing CCApi methods used by app.js: ${missingMethods.join(', ')}`,
);

assert(bridgeJs.includes('window.CCApi = {'), 'Tauri bridge must expose window.CCApi');
assert(!bridgeJs.includes('127.0.0.1:18081'), 'Tauri bridge must not call the retired Python admin server');
assert(appJs.includes('CCApi.submitFeedback'), 'Feedback UI must remain wired to the API bridge');
assert(appJs.includes('applyProviderToDesktop'), 'One-click provider apply flow must remain wired');

assert(!releaseWorkflow.includes('actions/setup-python'), 'Release workflow must not install Python');
assert(!releaseWorkflow.includes('requirements.txt'), 'Release workflow must not install Python requirements');
assert(windowsBuild.includes('pnpm tauri build'), 'Windows helper must build Tauri artifacts');
assert(!windowsBuild.toLowerCase().includes('pyinstaller'), 'Windows helper must not call PyInstaller');
assert(tauriConfig.includes('"installMode": "currentUser"'), 'Windows Tauri installer must stay per-user');

console.log(`Static contracts passed for ${calledMethods.size} CCApi methods.`);
