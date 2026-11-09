const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { execFileSync } = require('node:child_process');
const { transform } = require('./generate.cjs');

function generate(check = false) {
  const raw = fs.mkdtempSync(path.join(os.tmpdir(), 'safe-cex-balance-'));
  const destination = path.resolve(__dirname, '../../packages/safe-cex/src/generated');
  execFileSync('cargo', ['run', '--locked', '-j', '2', '-q', '-p', 'safe-cex-types', '--example', 'generate_balance', '--', raw], {
    cwd: path.resolve(__dirname, '..'), stdio: ['ignore', 'ignore', 'inherit'],
  });
  fs.mkdirSync(destination, { recursive: true });
  for (const name of ['Balance', 'CoinBalance']) {
    const body = transform(fs.readFileSync(path.join(raw, `${name}.ts`), 'utf8'), {}, `${name}.ts`).source;
    const declaration = name === 'Balance' ? `import type { CoinBalance } from './CoinBalance';\n${body}` : body;
    assert.equal(fs.readFileSync(path.join(__dirname, 'bindings', `${name}.ts`), 'utf8'), declaration,
      `${name} canonical bindings are stale; run the shared Rust generator first`);
    const binding = `../../../../sdk-rs/safe-cex-types/bindings/${name}`;
    const output = `// Generated from the Rust Balance slice; do not edit.\n`
      + (name === 'Balance' ? `import type { Balance } from '${binding}';\n` : '')
      + `export type { ${name} } from '${binding}';\n`
      + (name === 'Balance' ? `\n// Native initial state, not evidence that an account is loaded.\nexport const emptyBalance: Balance = ${fs.readFileSync(path.join(raw, 'empty.json'), 'utf8')};\n` : '');
    const filename = path.join(destination, `${name}.ts`);
    if (check) assert.equal(fs.readFileSync(filename, 'utf8'), output, `${name} generated output is stale`);
    else fs.writeFileSync(filename, output);
  }
}
if (require.main === module) {
  assert(process.argv.slice(2).every(arg => arg === '--check'), 'expected only --check');
  generate(process.argv.includes('--check'));
}
module.exports = { generate };
