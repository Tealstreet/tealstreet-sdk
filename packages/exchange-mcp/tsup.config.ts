import { copyFileSync, mkdirSync } from 'node:fs';
import { createRequire } from 'node:module';
import path from 'node:path';

import { defineConfig } from 'tsup';

const require = createRequire(import.meta.url);
const POSEIDON_WASM = path.join(
  path.dirname(require.resolve('@tealstreet/poseidon-crypto/package.json')),
  'src/wasm/poseidon_crypto.wasm',
);
const RUST_ENGINE = path.resolve('../../sdk-rs/hosts/wasm/pkg/node');

// One self-contained file, so `npx @tealstreet/exchange-mcp` installs nothing else.
export default defineConfig({
  // Named so both land flat in dist/, where the probe shares index.js's __dirname.
  entry: {
    index: 'src/index.ts',
    'poseidon-probe': 'scripts/poseidon-probe.ts',
    'rust-probe': 'scripts/rust-probe.ts',
  },
  format: ['esm'],
  platform: 'node',
  target: 'node20',
  noExternal: [/.*/],
  splitting: false,
  clean: true,
  // Read by scripts/third-party-licenses.mjs, then deleted.
  metafile: true,
  // ESM has no require or __dirname; bundled CommonJS uses the first, and
  // poseidon-crypto's node loader finds its WASM through the second.
  banner: {
    js: [
      '#!/usr/bin/env node',
      "import { createRequire as __mcpCreateRequire } from 'node:module'; const require = __mcpCreateRequire(import.meta.url);",
      "import { fileURLToPath as __toPath } from 'node:url'; import { dirname as __dirnameOf } from 'node:path';",
      'const __filename = __toPath(import.meta.url); const __dirname = __dirnameOf(__filename);',
    ].join('\n'),
  },
  onSuccess: async () => {
    mkdirSync('dist/wasm', { recursive: true });
    copyFileSync(POSEIDON_WASM, 'dist/wasm/poseidon_crypto.wasm');
    mkdirSync('dist/wasm/rust', { recursive: true });
    copyFileSync(path.join(RUST_ENGINE, 'safe_cex_wasm.js'), 'dist/wasm/rust/safe_cex_wasm.cjs');
    copyFileSync(path.join(RUST_ENGINE, 'safe_cex_wasm_bg.wasm'), 'dist/wasm/rust/safe_cex_wasm_bg.wasm');
  },
});
