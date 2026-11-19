import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { runInNewContext } from 'node:vm';
import { expectedGateCases, gateCaseKey, replayMatrix, validateGateCoverage } from './ci-gate-suites.mjs';
import { venueFamily } from '../hosts/wasm/js/venues.mjs';

const source = readFileSync(new URL('../../.github/workflows/rust.yml', import.meta.url), 'utf8');
const job = name => {
  const match = new RegExp(`^  ${name}:\\n([\\s\\S]*?)(?=^  [a-z][a-z0-9-]*:\\n|$(?![\\s\\S]))`, 'm').exec(source);
  assert(match, `missing ${name}`);
  return match[1];
};
const suites = ['binance', 'binancespot', 'binanceinverse', 'binancepm', 'binanceoptions'];

test('five isolated Binance rows preserve the original eleven exact replay identities', () => {
  const original = [
    ['binance', 'fixtures/binance-public.jsonl.gz', 'BTCUSDT,ETHUSDT,SOLUSDT'],
    ['binance', 'tapes/binance-private.jsonl', ''],
    ['binance', 'fixtures/binance-scripted-lifecycle-private.jsonl', 'BTCUSDT'],
    ['binancespot', 'fixtures/binancespot-public.jsonl.gz', 'BTCUSDT,ETHUSDT'],
    ['binancespot', 'tapes/binancespot-private.jsonl', ''],
    ['binanceinverse', 'fixtures/binanceinverse-public.jsonl.gz', 'BTCUSD_PERP,ETHUSD_PERP'],
    ['binanceinverse', 'tapes/binanceinverse-private.jsonl', ''],
    ['binancepm', 'fixtures/binance-public.jsonl.gz', 'BTCUSDT,ETHUSDT,SOLUSDT'],
    ['binancepm', 'tapes/binancepm-private.jsonl', ''],
    ['binanceoptions', 'tapes/binanceoptions.jsonl', ''],
    ['binanceoptions', 'tapes/binanceoptions-private.jsonl', ''],
  ].map(([venue, tape, books]) => gateCaseKey({ venue, tape: `sdk-rs/harness/${tape}`, books }));
  assert.deepEqual(suites.flatMap(expectedGateCases).map(gateCaseKey).sort(), original.sort());
  for (const row of replayMatrix(suites, venueFamily)) {
    assert.equal(row.family, 'binance');
    const cases = expectedGateCases(row.venue);
    assert.equal(cases.length, row.venue === 'binance' ? 3 : 2);
    assert.deepEqual(validateGateCoverage(row.venue, cases), []);
    assert(validateGateCoverage(row.venue, cases.slice(1)).some(error => error.startsWith('missing gate case:')));
  }
});

test('combined consumer assertions run separately from serial, sealed measurement rows', () => {
  const wasmConsumers = job('replay-consumers');
  const nativeConsumers = job('replay-native-consumers');
  for (const consumer of [wasmConsumers, nativeConsumers]) {
    assert.match(consumer, /needs: selection\n    if: needs.selection.outputs.replayRequired == 'true'/);
    assert.doesNotMatch(consumer, /needs:.*(?:replay-native-consumers|replay-consumers)/);
  }
  assert.doesNotMatch(wasmConsumers, /session-adapter-fixture|rust:test-server-adapter|rust:test-frame-wire/);
  assert.doesNotMatch(nativeConsumers, /rust:build-wasm|test-wasm/);
  const consumers = wasmConsumers + nativeConsumers;
  for (const command of [
    'rust:test-frame-wire',
    'cargo build --locked --manifest-path sdk-rs/Cargo.toml -p safe-cex-native --example session-adapter-fixture',
    'rust:test-server-adapter', 'rust:build-wasm',
    'node --test sdk-rs/harness/capabilities-combined.integration.mjs',
    'rust:wasm-budget --combined', 'rust:measure-browser --archive combined',
    'workspace @tealstreet/safe-cex test-wasm',
    'workspace @tealstreet/safe-cex-server test-wasm',
    'workspace @tealstreet/exchange-mcp test-wasm', 'workspace @tealstreet/cli test-wasm',
    'workspace @tealstreet/safe-cex-react-native test-unit',
    'workspace @tealstreet/exchange-mcp build', 'workspace @tealstreet/exchange-mcp smoke',
    'rust:test-product-semantics', 'workspace @tealstreet/safe-cex-wasm test',
  ]) assert(consumers.includes(command), command);
  assert.match(consumers, /name: measured-browser-wasm-combined/);
  assert.doesNotMatch(consumers, /cpu-ci\.mjs|rust:gate:record|continue-on-error/);
  const measurement = job('gate');
  assert.doesNotMatch(measurement, /needs:.*replay-consumers|test-server-adapter|test-frame-wire|test-wasm|test-product-semantics/);
  for (const suite of suites) assert(measurement.includes(`- venue: ${suite}\n`), suite);
  assert.match(measurement, /--venue \$\{\{ matrix.family \|\| matrix.venue \}\}/);
  assert.match(measurement, /--command prepare --suite '\$\{\{ matrix.venue \}\}'/);
  assert.match(measurement, /--command collect --suite '\$\{\{ matrix.venue \}\}'/);
  assert.doesNotMatch(measurement, /rust:gate:record[^\n]*&\s*$/m);
});

test('family comparisons build the same Binance family even against a pre-sharding baseline', () => {
  const collector = readFileSync(new URL('./collect-cpu-family.py', import.meta.url), 'utf8');
  assert.match(collector, /'build.sh'|harness\/build.sh/);
  assert.match(collector, /'--venue', authority\['family'\]/);
  assert.doesNotMatch(collector, /'--venue', authority\['suite'\]/);
});

for (const suite of [...suites, 'bitget', 'bitgetusdc']) {
  test(`${suite} retains the original diagnostic sweep's private pair count`, () => {
    const measurement = job('gate');
    const block = measurement.split(/^          - venue: /m).slice(1).find(row => row.startsWith(`${suite}\n`));
    assert(block, suite);
    const matrix = { venue: suite, family: /^            family: (\S+)/m.exec(block)?.[1] };
    const expression = /^      PRIVATE_REPLAY_RUNS: \$\{\{ (.*) \}\}$/m.exec(measurement)?.[1];
    assert(expression, 'evaluate the actual workflow condition');
    const evaluate = (head_ref, ref_name = 'master') => runInNewContext(expression, {
      matrix, github: { head_ref, ref_name }, format: (pattern, value) => pattern.replace('{0}', value),
    }, { timeout: 100 });
    assert.equal(evaluate('ordinary-change'), '1');
    const sweep = `wip/harness-tape-time-${suite.startsWith('binance') ? 'binance' : 'bitget'}`;
    assert.equal(evaluate(sweep), '5');
    assert.equal(evaluate(`wip/harness-tape-time-${suite}`),
      suite.startsWith('binance') && suite !== 'binance' ? '1' : '5', 'per-product branches retain their former behavior');
    assert.equal(evaluate('', sweep), '5', 'pushes use ref_name just like PR head_ref');
    assert.equal(evaluate('ordinary-change', sweep), '1', 'head_ref retains precedence');
  });
}
