import test from 'node:test';
import assert from 'node:assert/strict';
import { createHmac } from 'node:crypto';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import { buildAsterTape, ASTER_EPOCH } from './private-tape-asterdex.mjs';
import { createTape } from './tape.mjs';
import { venueOf } from './venues.mjs';

const root = fileURLToPath(new URL('../../', import.meta.url));

test('Aster strict tape rejects an economically changed, correctly signed order', () => {
  const row = buildAsterTape().find(
    (record) => record.type === 'http' && record.method === 'POST' && new URL(record.url).pathname.endsWith('/order'),
  );
  const tape = createTape([row], venueOf('asterdex'), true);
  const send = (quantity) => {
    const parsed = new URL(row.url);
    parsed.searchParams.set('quantity', quantity);
    parsed.searchParams.set('recvWindow', '20000');
    parsed.searchParams.set('timestamp', String(Date.now()));
    const signature = createHmac('sha256', 'replay-secret')
      .update([...parsed.searchParams].map(([k, v]) => `${k}=${v}`).join('&'))
      .digest('hex');
    parsed.searchParams.set('signature', signature);
    return tape.respond('POST', parsed.href, undefined, { 'X-MBX-APIKEY': 'replay-key' });
  };
  assert.equal(send('0.004').status, 599);
  assert.equal(tape.unusedWrites(), 1);
  assert.equal(send('0.003').status, 200);
  assert.equal(tape.unusedWrites(), 0);
});

for (const secondary of [false, true])
  test(
    `Aster V3 native builder, STP and history work against independent payload expectations${secondary ? ' with a separate read agent' : ''}`,
    { timeout: 45000 },
    () => {
      const directory = mkdtempSync(join(tmpdir(), 'asterdex-parity-'));
      try {
        const tape = join(directory, 'native.jsonl'),
          out = join(directory, 'result.json');
        writeFileSync(
          tape,
          `${buildAsterTape({ v3: true, native: true, secondary })
            .map((record) => JSON.stringify(record))
            .join('\n')}\n`,
        );
        const result = spawnSync(
          'corepack',
          [
            'yarn',
            '-s',
            'rust:replay',
            '--impl',
            'rust',
            '--venue',
            'asterdex',
            '--tape',
            tape,
            '--books',
            '',
            '--out',
            out,
          ],
          { cwd: root, encoding: 'utf8', timeout: 40000, env: { ...process.env, ESBUILD_WORKER_THREADS: '0' } },
        );
        assert.ifError(result.error);
        assert.equal(result.status, 0, `${result.stdout}\n${result.stderr}`);
        const replay = JSON.parse(readFileSync(out, 'utf8'));
        for (const field of ['unserved', 'unusedWrites', 'invalidSignatures']) assert.equal(replay[field], 0, field);
        assert.deepEqual(replay.protocolErrors, []);
        assert.equal(replay.commands.length, 15);
        assert.deepEqual(replay.commands[0], { ok: ['1002'] });
        assert.deepEqual(replay.commands[1], { ok: ['1003'] });
        assert.deepEqual(replay.commands[2], { ok: [] });
        const fills = replay.commands[3].ok;
        assert.deepEqual(
          fills.map(({ id, realizedPnl, positionSide, maker }) => ({ id, realizedPnl, positionSide, maker })),
          [
            { id: '3', realizedPnl: 3, positionSide: 'both', maker: false },
            { id: '1', realizedPnl: 0, positionSide: 'both', maker: true },
            { id: '2', realizedPnl: 2, positionSide: 'both', maker: false },
          ],
        );
        assert.deepEqual(replay.commands[4], { ok: 'EXPIRE_MAKER' });
        assert.deepEqual(replay.commands[5], { ok: [] });
        assert.deepEqual(
          replay.commands[6].ok.map(({ id, status, amount, filled, averagePrice }) => ({
            id,
            status,
            amount,
            filled,
            averagePrice,
          })),
          [{ id: '1001', status: 'closed', amount: 1, filled: 1, averagePrice: 89.6 }],
        );
        assert.deepEqual(replay.commands[7].ok, [
          {
            symbol: 'BTCUSDT',
            incomeType: 'FUNDING_FEE',
            income: -0.25,
            asset: 'USDT',
            timestamp: ASTER_EPOCH + 1000,
            tranId: '9001',
            category: 'funding',
          },
        ]);
        assert.deepEqual(replay.commands[8].ok, [
          { id: '28457', symbol: 'BTCUSDT', price: 89, qty: 0.4, quoteQty: 35.6, timestamp: ASTER_EPOCH, side: 'sell' },
        ]);
        assert.deepEqual(replay.commands[9].ok, [
          { timestamp: ASTER_EPOCH / 1000, open: 89, high: 91, low: 88, close: 90, volume: 10 },
        ]);
        assert.match(replay.commands[10].error, /callbackRate/);
        assert.match(replay.commands[11].error, /client order IDs/);
        assert.deepEqual(replay.commands[12], { ok: { userId: 'documented-fixture' } });
        assert.deepEqual(replay.commands[13], { ok: { buyLeverage: 10, sellLeverage: 10 } });
        assert.deepEqual(replay.commands[14], {
          ok: {
            bids: [
              { price: 89, amount: 2, total: 178 },
              { price: 88, amount: 3, total: 442 },
            ],
            asks: [
              { price: 91, amount: 4, total: 364 },
              { price: 92, amount: 5, total: 824 },
            ],
          },
        });
        assert.deepEqual(
          replay.account.fills.map(({ id, amount, price }) => ({ id, amount, price })),
          [
            { id: '1001', amount: 0.4, price: 89 },
            { id: '1001', amount: 0.6, price: 90 },
          ],
        );
        assert(
          replay.writes.some(
            (request) =>
              request.includes('stpMode=EXPIRE_BOTH') &&
              request.includes('feeRate=0.00005') &&
              request.includes('builder='),
          ),
        );
        assert(
          replay.writes.some((request) => request.includes('/stpMode?') && request.includes('stpMode=EXPIRE_TAKER')),
        );
      } finally {
        rmSync(directory, { recursive: true, force: true });
      }
    },
  );
