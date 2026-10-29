import assert from 'node:assert/strict';
import test from 'node:test';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { deterministicReplay } from './polling.mjs';
import { Tape } from './tape.mjs';
import { VENUES } from './venues.mjs';

const generator = fileURLToPath(new URL('./private-tape-weex.mjs', import.meta.url));
const ORDERS = 'https://api-contract.weex.com/capi/v3/openOrders?limit=100&page=0';
const ALGOS = 'https://api-contract.weex.com/capi/v3/openAlgoOrders?limit=100&page=1';

const weexTape = () => {
  const directory = mkdtempSync(join(tmpdir(), 'weex-tape-'));
  try {
    const out = join(directory, 'weex-private.jsonl');
    const generated = spawnSync(process.execPath, [generator, '--out', out], { encoding: 'utf8', cwd: directory });
    assert.equal(generated.status, 0, generated.stderr);
    return readFileSync(out, 'utf8').trim().split('\n').map((line) => JSON.parse(line));
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
};
const ids = (response) => JSON.parse(response.body).map((row) => row.orderId ?? row.algoId);

test('all 51 covered venue configurations use deterministic private replay clocks', () => {
  const covered = [
    'apexomni', 'asterdex', 'backpack', 'binance', 'binanceinverse',
    'binanceoptions', 'binancepm', 'binancespot', 'bingx', 'bitget', 'bitgetinverse',
    'bitgetusdc', 'bitrue', 'bitunix', 'blofin', 'breakoutprop',
    'bybit', 'bybitdemo', 'bybitinverse', 'bybitspot', 'coinbase',
    'coinbasespot', 'cryptocom', 'grvt', 'hyperliquid', 'hyperliquidspot',
    'hyrotrader', 'kodiakfi', 'kraken', 'krakenspot', 'kucoin',
    'kucoinspot', 'lighter', 'lighter-robinhood', 'mexc', 'mode',
    'okx', 'okxdemo', 'okxeu', 'okxeudemo', 'orderly',
    'paradex', 'phemex', 'polymarket', 'salsadex', 'tealstreet-dex',
    'txflow', 'weex', 'what', 'woo', 'woofi',
  ];
  assert.deepEqual(Object.keys(VENUES).sort(), covered);
  for (const name of covered) assert.equal(deterministicReplay(VENUES[name], true), true, name);
});

test('a WEEX order snapshot is served only after its command write is acknowledged', () => {
  const records = weexTape();
  const tape = new Tape(records, { isWrite: VENUES.weex.isWrite }, true, 2000, 'ts');
  const cancel = records.find((record) => record.method === 'DELETE' && record.url.endsWith('/capi/v3/batchOrders'));
  tape.time = 4100;
  tape.commandIndex = 3;
  tape.settledCommands = 2;
  // A poll racing the cancel command still reads the open caller order.
  assert.deepEqual(ids(tape.respond('GET', ORDERS)), ['702345678901234560', '702345678901234561', '702345678901234562']);
  assert.deepEqual(ids(tape.respond('GET', ALGOS)), ['702345678901234563']);
  assert.equal(tape.respond('DELETE', cancel.url, cancel.requestBody).status, 200);
  assert.deepEqual(ids(tape.respond('GET', ORDERS)), ['702345678901234560', '702345678901234562']);
  assert.deepEqual(ids(tape.respond('GET', ALGOS)), []);
});

// TS ticks every 2 s and the native account cycle runs at 250 ms past it; a
// command at either tie would order the poll and the command arbitrarily.
test('no WEEX command shares a tape time with a periodic REST read', () => {
  for (const { t } of weexTape().filter((record) => record.type === 'command')) {
    assert.notEqual(t % 2000, 0, `command at ${t}`);
    assert.notEqual(t % 2000, 250, `command at ${t}`);
  }
});
