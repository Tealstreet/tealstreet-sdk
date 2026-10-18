import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { dirname, join, normalize } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import vm from 'node:vm';
import ts from 'typescript';

const root = fileURLToPath(new URL('../../../', import.meta.url));
const environmentKeys = ['NEXT_PUBLIC_HYROTRADER_SIMULATOR_REST_URL', 'NEXT_PUBLIC_HYROTRADER_SIMULATOR_PUBLIC_WS_URL', 'NEXT_PUBLIC_HYROTRADER_SIMULATOR_PRIVATE_WS_URL', 'NEXT_PUBLIC_HYROTRADER_PROD_REST_URL', 'NEXT_PUBLIC_HYROTRADER_PROD_PUBLIC_WS_URL', 'NEXT_PUBLIC_HYROTRADER_PROD_PRIVATE_WS_URL'];
const configured = Object.fromEntries(environmentKeys.map(key => [key, process.env[key] ?? '']));
const cache = new Map();
function load(file) {
  if (cache.has(file)) return cache.get(file);
  const exports = {};
  cache.set(file, exports);
  const text = readFileSync(join(root, 'packages/safe-cex/src', file), 'utf8');
  const source = ts.createSourceFile(file, text, ts.ScriptTarget.Latest, true);
  // The tables import only shared enum values; avoid evaluating unrelated host modules.
  let program = file === 'types.ts' ? source.statements.filter(ts.isEnumDeclaration).map(n => n.getText(source)).join('\n') : text;
  if (file === 'exchanges/bybit/bybit.types.ts') program += '\nexport { normalizeConfiguredOrigin, restOriginToWsUrl };';
  vm.runInNewContext(ts.transpileModule(program, { compilerOptions: { module: ts.ModuleKind.CommonJS } }).outputText, {
    exports, process: { env: configured }, require: name => load(normalize(join(dirname(file), name)) + '.ts'),
  });
  return exports;
}
const linear = load('exchanges/bybit/bybit.types.ts');
const spot = load('exchanges/bybit/bybitspot.types.ts');
const rate_limits = load('exchanges/bybit/bybit.rateLimits.ts');
const origins = [null, '', 'https://example.test///', ' HTTP://EXAMPLE.test/ ', 'ftp://example.test/', 'httpOdd', '雪/'];
// Every BMP code point exercises JS trim versus Rust's deliberately narrower predicate.
for (let code = 0; code <= 0xffff; code++) {
  if (code >= 0xd800 && code <= 0xdfff) continue; // Rust String cannot represent lone UTF-16 surrogates.
  const c = String.fromCharCode(code);
  origins.push(`${c}https://example.test///${c}`);
}
const helpers = origins.map(origin => {
  const normalized = linear.normalizeConfiguredOrigin(origin ?? undefined);
  return [normalized, linear.restOriginToWsUrl(normalized, '/v5/private')];
});
const expected = JSON.parse(JSON.stringify({ linear, spot, rate_limits, helpers }));
assert.equal(origins.length,63495,'DATA-CARDINALITY source origins');
const run = spawnSync(process.env.CARGO ?? 'cargo', ['run', '--quiet', '--locked', '--jobs', '1', '--manifest-path', 'sdk-rs/Cargo.toml', '-p', 'safe-cex-venue-bybit', '--example', 'data'], {
  cwd: root, env: { ...process.env, ...configured }, encoding: 'utf8', input: JSON.stringify(origins), maxBuffer: 32 * 1024 * 1024,
});
assert.equal(run.status, 0, run.error?.message ?? run.stderr);
assert.deepEqual(JSON.parse(run.stdout), expected, 'Bybit tables/helpers differ from live TypeScript');
console.log(`Bybit live data equivalence: linear/spot exports, rate table, ${origins.length} origin cases.`);
