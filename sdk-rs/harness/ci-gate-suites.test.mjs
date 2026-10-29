import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import test from 'node:test';
import { expectedGateCases, gateCaseKey, GATE_SUITES, validateGateCoverage, replayMatrix } from './ci-gate-suites.mjs';
import { venueFamily } from '../hosts/wasm/js/venues.mjs';

const copy = (suite) => structuredClone(expectedGateCases(suite));

test('suite registry preserves every primary, alias, profile and network replay', () => {
  const exceptional = { binance: 11, bybit: 10, okx: 8, orderly: 15, coinbase: 9, asterdex: 3, lighter: 3, 'lighter-robinhood': 3, bitget: 3, hyperliquid: 3 };
  assert.equal(GATE_SUITES.length, 32);
  assert.equal(GATE_SUITES.reduce((total, suite) => total + expectedGateCases(suite).length, 0), 112);
  for (const suite of GATE_SUITES) {
    const cases = expectedGateCases(suite);
    assert.equal(cases.length, exceptional[suite] ?? 2, suite);
    assert.equal(new Set(cases.map(gateCaseKey)).size, cases.length, suite);
    assert.deepEqual(validateGateCoverage(suite, [...cases].reverse()), [], suite);
    assert(Object.isFrozen(cases));
    assert(cases.every(Object.isFrozen));
  }
});

test('family profiles pin their exact tape paths and private books', () => {
  assert.deepEqual(expectedGateCases('binance').find((item) => item.venue === 'binancepm' && item.books), {
    venue: 'binancepm', tape: 'sdk-rs/harness/fixtures/binance-public.jsonl.gz', books: 'BTCUSDT,ETHUSDT,SOLUSDT',
  });
  assert(expectedGateCases('binance').some((item) => item.tape === 'sdk-rs/harness/fixtures/binance-scripted-lifecycle-private.jsonl' && item.books === 'BTCUSDT'));
  assert.deepEqual(expectedGateCases('okx').filter((item) => item.books).map(({ venue, tape }) => [venue, tape]), [
    ['okx', 'sdk-rs/harness/fixtures/okx-public.jsonl.gz'],
    ['okx', 'sdk-rs/harness/fixtures/okx-inverse-public.jsonl.gz'],
    ['okxeu', 'sdk-rs/harness/fixtures/okxeu-xperp-public.jsonl.gz'],
    ['okxeu', 'sdk-rs/harness/fixtures/okxeu-public.jsonl.gz'],
  ]);
  const orderly = expectedGateCases('orderly');
  for (const venue of ['orderly', 'tealstreet-dex', 'kodiakfi', 'woofi', 'mode', 'salsadex', 'what']) {
    assert(orderly.some((item) => item.venue === venue && item.tape === `sdk-rs/harness/tapes/${venue}-testnet-private.jsonl` && item.books === ''), venue);
    assert(orderly.some((item) => item.venue === venue && item.tape === `sdk-rs/harness/tapes/${venue}-private.jsonl` && item.books === ''), venue);
  }
  const coinbase = expectedGateCases('coinbase');
  for (const profile of ['intx', 'cfm', 'spot'])
    for (const algorithm of ['ES256', 'EdDSA'])
      assert(coinbase.some((item) => item.venue === 'coinbase' && item.tape === `sdk-rs/harness/tapes/coinbase-${profile}-${algorithm}-private.jsonl` && item.books === ''));
  for (const algorithm of ['ES256', 'EdDSA'])
    assert(coinbase.some((item) => item.venue === 'coinbasespot' && item.tape === `sdk-rs/harness/tapes/coinbasespot-spot-${algorithm}-private.jsonl` && item.books === ''));
  for (const suite of ['lighter', 'lighter-robinhood'])
    assert(expectedGateCases(suite).some((item) => item.venue === suite && item.tape === 'sdk-rs/harness/tapes/lighter-testnet-private.jsonl' && item.books === ''));
  assert.deepEqual(expectedGateCases('bitgetusdc'), [
    { venue: 'bitgetusdc', tape: 'sdk-rs/harness/fixtures/bitgetusdc-public.jsonl.gz', books: 'BTCUSDC,ETHUSDC' },
    { venue: 'bitgetusdc', tape: 'sdk-rs/harness/tapes/bitgetusdc-private.jsonl', books: '' },
  ]);
});

test('coverage rejects missing, duplicate and substituted identities even when total count matches', () => {
  for (const suite of GATE_SUITES) {
    const cases = copy(suite);
    assert(validateGateCoverage(suite, cases.slice(1)).some((error) => error.startsWith('missing gate case:')), suite);
    cases[1] = cases[0];
    const errors = validateGateCoverage(suite, cases);
    assert(errors.some((error) => error.startsWith('duplicate gate case')), suite);
    assert(errors.some((error) => error.startsWith('missing gate case:')), suite);
  }
});

test('wrong venue, tape, profile, testnet path or exact books cannot replace a case', () => {
  for (const [suite, field, replacement] of [
    ['bitgetusdc', 'venue', 'bitget'],
    ['okx', 'tape', 'sdk-rs/harness/fixtures/okxeu-public.jsonl.gz'],
    ['coinbase', 'tape', 'sdk-rs/harness/tapes/coinbase-intx-RS256-private.jsonl'],
    ['lighter-robinhood', 'tape', 'sdk-rs/harness/tapes/lighter-robinhood-testnet-private.jsonl'],
    ['binance', 'books', 'BTCUSDT,ETHUSDT'],
    ['kucoinspot', 'books', 'ETH-USDT,BTC-USDT'],
    ['paradex', 'books', ''],
  ]) {
    const cases = copy(suite);
    cases[0][field] = replacement;
    const errors = validateGateCoverage(suite, cases);
    assert(errors.some((error) => error.startsWith('unexpected gate case:')), `${suite} ${field}`);
    assert(errors.some((error) => error.startsWith('missing gate case:')), `${suite} ${field}`);
  }
  const cases = copy('bitrue');
  cases[1].books = 'E-BTC-USDT';
  assert(validateGateCoverage('bitrue', cases).some((error) => error.startsWith('unexpected gate case:')));
});

test('unknown suites, unexpected cases and malformed or absent paths fail closed', () => {
  assert.throws(() => expectedGateCases('unknown'), /Unknown replay gate suite/);
  assert(validateGateCoverage('unknown', []).some((error) => error.includes('Unknown replay gate suite')));
  assert.deepEqual(validateGateCoverage('bitrue', undefined), ['gate cases must be an array']);
  assert(validateGateCoverage('bitrue', [...copy('bitrue'), { venue: 'extra', tape: 'extra', books: '' }]).some((error) => error.startsWith('unexpected gate case:')));
  for (const item of [null, [], {}, { venue: 'bitrue', books: '' }, { venue: 'bitrue', tape: '', books: '' }, { venue: 'bitrue', tape: 'path' }, { venue: 1, tape: 'path', books: '' }]) {
    assert.throws(() => gateCaseKey(item));
    assert(validateGateCoverage('bitrue', [item]).some((error) => error.startsWith('invalid gate case at index')));
  }
});

// Read the actual CI matrix/run blocks and expand their shell loops with a
// mocked yarn function. No generator, build, network request or gate executes.
// This checks the registry against all CLI identities, not only family counts.
const workflowCases = (source, name) => {
  const match = new RegExp(`^  ${name}:\\n([\\s\\S]*?)(?=^  [a-z][a-z-]*:\\n|$(?![\\s\\S]))`, 'm').exec(source);
  assert(match, `${name} job exists`);
  const job = match[1];
  const matrixStart = job.indexOf('      matrix:\n');
  const matrixEnd = job.indexOf('\n    env:', matrixStart);
  assert(matrixStart >= 0 && matrixEnd > matrixStart, 'matrix include table exists');
  let rows;
  if (name === 'replay') {
    assert.match(job.slice(matrixStart, matrixEnd), /include: \$\{\{ fromJSON\(needs\.selection\.outputs\.replay\) \}\}/);
    rows = replayMatrix(GATE_SUITES, venueFamily);
  } else {
    rows = job.slice(matrixStart, matrixEnd).split(/^          - venue: /m).slice(1).map((block) => {
    const [venue, ...lines] = block.split('\n');
    const fields = { venue: venue.trim() };
    for (const line of lines) {
      const match = /^            ([a-z_]+): (.*)$/.exec(line);
      if (match) fields[match[1]] = match[2].trim().replace(/^(['"])(.*)\1$/, '$2');
    }
    return fields;
  });
  }
  const steps = job.split(/^      - /m).slice(1);
  const selected = (condition, suite) => !condition || condition.split(/\s*\|\|\s*/).some((part) =>
    part.split(/\s*&&\s*/).every((clause) => {
      const match = /^matrix\.venue\s*(==|!=)\s*'([^']+)'$/.exec(clause.trim());
      assert(match, `supported replay step condition: ${clause}`);
      return match[1] === '==' ? suite === match[2] : suite !== match[2];
    }));
  const results = {};
  for (const row of rows) {
    const cases = [];
    for (const step of steps) {
      if (!/yarn(?:\s+-[^\s]+)*\s+rust:gate(?::record)?(?:\s|$)/.test(step)) continue;
      const condition = /^        if: (.*)$/m.exec(step)?.[1];
      if (!selected(condition, row.venue)) continue;
      const lines = step.split('\n');
      const index = lines.findIndex((line) => line.startsWith('        run: '));
      assert(index >= 0, 'gate step has run');
      const inline = lines[index].slice('        run: '.length);
      const script = (inline === '|' ? lines.slice(index + 1).filter((line) => line.startsWith('          ') || !line.trim()).map((line) => line.slice(10)).join('\n') : inline)
        .replace(/\$\{\{\s*matrix\.([a-z_]+)\s*\}\}/g, (_whole, key) => {
          assert(Object.hasOwn(row, key), `matrix field ${key}`);
          return row[key];
        });
      const mock = `set -e\ncorepack() { [[ \"$1\" == yarn ]] || return 1; shift; yarn \"$@\"; }\nyarn() {\n while [[ $# -gt 0 && \"$1\" == -* ]]; do shift; done\n if [[ "$1" == rust:gate || "$1" == rust:gate:record ]]; then\n shift\n printf 'CASE\\0'\n printf '%s\\0' "$@"\n printf 'END\\0'\n fi\n return 0\n}\n`;
      const result = spawnSync('/bin/bash', ['--noprofile', '--norc'], { input: mock + script + '\n', encoding: 'utf8', timeout: 10000, env: { PATH: '/nonexistent' } });
      assert.ifError(result.error);
      assert.equal(result.status, 0, `${row.venue}: ${result.stderr}`);
      const tokens = result.stdout.split('\0');
      assert.equal(tokens.pop(), '');
      while (tokens.length) {
        assert.equal(tokens.shift(), 'CASE');
        const end = tokens.indexOf('END');
        assert(end >= 0);
        const args = tokens.splice(0, end);
        tokens.shift();
        const argument = (name) => {
          assert.equal(args.filter((value) => value === name).length, 1, `${row.venue} requires one ${name}`);
          return args[args.indexOf(name) + 1];
        };
        cases.push({ venue: argument('--venue'), tape: argument('--tape'), books: argument('--books') });
      }
    }
    results[row.venue] = cases;
  }
  return results;
};

test('CI matrix and every expanded CLI case exactly match declared suite coverage', () => {
  const source = readFileSync(new URL('../../.github/workflows/rust.yml', import.meta.url), 'utf8');
  for (const name of ['replay', 'gate']) {
    const actual = workflowCases(source, name);
    assert.deepEqual(Object.keys(actual).sort(), [...GATE_SUITES].sort());
    for (const suite of GATE_SUITES) assert.deepEqual(validateGateCoverage(suite, actual[suite]), [], `${name}: ${suite}`);
  }
});

test('deferred strict results require mandatory final certification even after failures', () => {
  const source = readFileSync(new URL('../../.github/workflows/rust.yml', import.meta.url), 'utf8');
  const job = source.slice(source.indexOf('\n  gate:'));
  const verifier = job.indexOf('name: Verify strict replay results and declared known failures');
  assert.ok(verifier > job.lastIndexOf('rust:gate:record --venue'));
  assert.match(job.slice(verifier), /if: always\(\)/);
  assert.match(job.slice(verifier), /--command collect --suite/);
  assert.doesNotMatch(job.slice(verifier), /rust:gate:verify --suite/);
  assert.doesNotMatch(job, /continue-on-error/);
  const report = readFileSync(new URL('./record-gate.mjs', import.meta.url), 'utf8');
  assert.match(report, /Strict replay did not produce a complete report/);
  assert.match(report, /certification remains pending/);
});
