import assert from 'node:assert/strict';
import test from 'node:test';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import { buildBitgetUtaTape, buildBitgetClassicTrailingTape, UTA_EPOCH } from './private-tape-bitget-uta.mjs';
import { createTape } from './tape.mjs';

const root = fileURLToPath(new URL('../../', import.meta.url));

test('UTA tape requires the documented trailing payload before consuming a write', () => {
  const records = buildBitgetUtaTape();
  for (const record of records.filter((r) => r.type === 'http')) {
    if (record.method === 'GET' && new URL(record.url).searchParams.get('type') === 'trailing_stop') {
      for (const row of JSON.parse(record.body).data) {
        assert.equal(row.trailingStopParams.trailType, 'spread');
        assert.equal(row.trailingStopParams.activationType, 'market');
        assert.equal(row.trailingStopParams.preOrderType, 'market');
      }
    }
    if (record.requestBody?.type === 'trailing_stop') {
      assert.equal(record.requestBody.trailingStopParams[0].trailType, 'distance');
    }
  }
  for (const record of records.filter((r) => r.type === 'ws')) {
    const message = JSON.parse(record.text);
    if (message.arg?.topic === 'strategy-order') {
      assert.equal(message.arg.symbol, 'default');
      assert(message.data.every((row) => row.type === 'tpsl' && !row.trailingStopParams));
    }
  }
  const native = records.find((r) => r.method === 'POST');
  const tape = createTape([native], { rust: 'bitget' }, true);
  const wrong = structuredClone(native.requestBody);
  wrong.trailingStopParams[0].trailVariance = '500';
  assert.equal(tape.respond('POST', native.url, wrong).status, 599);
  assert.equal(tape.unusedWrites(), 1);
  assert.equal(tape.respond('POST', native.url, native.requestBody).status, 200);
  assert.equal(tape.unusedWrites(), 0);
});

for (const usdc of [false, true]) {
  const venue = usdc ? 'bitgetusdc' : 'bitget';
  test(
    `${venue} UTA native ledger and trailing replay matches independent documented expectations`,
    { timeout: 45000 },
    () => {
      const dir = mkdtempSync(join(tmpdir(), `${venue}-uta-parity-`));
      try {
        const tapePath = join(dir, 'private.jsonl');
        const output = join(dir, 'result.json');
        const records = buildBitgetUtaTape({ usdc });
        writeFileSync(tapePath, `${records.map((r) => JSON.stringify(r)).join('\n')}\n`);
        const run = spawnSync(
          'corepack',
          [
            'yarn',
            '-s',
            'rust:replay',
            '--impl',
            'rust',
            '--venue',
            venue,
            '--tape',
            tapePath,
            '--books',
            '',
            '--out',
            output,
          ],
          {
            cwd: root,
            encoding: 'utf8',
            timeout: 40000,
            env: { ...process.env, ESBUILD_WORKER_THREADS: '0' },
          },
        );
        assert.ifError(run.error);
        assert.equal(run.status, 0, `${run.stdout}\n${run.stderr}`);
        const result = JSON.parse(readFileSync(output, 'utf8'));
        assert.equal(result.unserved, 0);
        assert.equal(result.unusedWrites, 0);
        assert.equal(result.missingWrites, 0);
        assert.equal(result.invalidSignatures, 0);
        assert.deepEqual(result.protocolErrors, []);
        assert.equal(result.commands.length, 5);
        const coin = usdc ? 'USDC' : 'USDT';
        const symbol = `BTC${coin}`;
        const ledger = result.commands[0].ok;
        assert.deepEqual(
          ledger.map(({ tranId, incomeType, income, category, fee, asset, timestamp }) => ({
            tranId,
            incomeType,
            income,
            category,
            fee,
            asset,
            timestamp,
          })),
          [
            {
              tranId: '9007199254740993:funding',
              incomeType: 'CONTRACT_MAIN_SETTLE_FEE_USER_OUT',
              income: -2,
              category: 'funding',
              fee: 0,
              asset: coin,
              timestamp: UTA_EPOCH - 1000,
            },
            {
              tranId: '2:other',
              incomeType: 'CLOSE_LONG',
              income: 10,
              category: 'other',
              fee: -0.1,
              asset: coin,
              timestamp: UTA_EPOCH - 1000,
            },
            {
              tranId: '3:transfer',
              incomeType: 'TRANSFER_IN',
              income: 5,
              category: 'transfer',
              fee: 0,
              asset: coin,
              timestamp: UTA_EPOCH - 1000,
            },
            {
              tranId: '4:commission',
              incomeType: 'ORDER_PLF_FEE_OUT',
              income: -0.2,
              category: 'commission',
              fee: -0.2,
              asset: coin,
              timestamp: UTA_EPOCH - 1000,
            },
          ],
        );
        assert(ledger.every((r) => r.symbol === symbol));
        assert.deepEqual(result.commands[1], { ok: ['trail-created'] });
        assert.match(result.commands[2].error, /invalid qty/);
        assert.deepEqual(result.commands[3], { ok: ['trail-amended'] });
        assert.deepEqual(result.commands[4], { ok: [] });
        // Every trailing row here came from staged native REST snapshots.
        // Neither discovery nor cancellation depends on an undocumented WS shape.
        assert.deepEqual(result.account.orders.map((o) => o.id).sort(), [
          'existing-stop',
          'trail-amended',
          'trail-final',
        ]);
        assert.deepEqual(
          result.account.orders.map(
            ({ id, type, status, side, price, amount, filled, remaining, reduceOnly, trigger }) => ({
              id,
              type,
              status,
              side,
              price,
              amount,
              filled,
              remaining,
              reduceOnly,
              trigger,
            }),
          ),
          [
            {
              id: 'existing-stop',
              type: 'stop_market',
              status: 'open',
              side: 'sell',
              price: 90,
              amount: 0.01,
              filled: 0,
              remaining: 0.01,
              reduceOnly: true,
              trigger: 'last_price',
            },
            {
              id: 'trail-amended',
              type: 'trailing_stop_market',
              status: 'open',
              side: 'sell',
              price: 95,
              amount: 0.02,
              filled: 0,
              remaining: 0.02,
              reduceOnly: true,
              trigger: 'last_price',
            },
            {
              id: 'trail-final',
              type: 'trailing_stop_market',
              status: 'open',
              side: 'sell',
              price: 96,
              amount: 0.03,
              filled: 0,
              remaining: 0.03,
              reduceOnly: true,
              trigger: 'last_price',
            },
          ],
        );
        assert(
          result.requests.includes(
            `GET https://api.bitget.com/api/v3/trade/unfilled-strategy-orders?category=${coin}-FUTURES&type=trailing_stop`,
          ),
        );
        const final = result.account.orders.find((o) => o.id === 'trail-final');
        assert(final, JSON.stringify(result.account.orders));
        assert.equal(final.symbol, symbol);
        assert.equal(final.type, 'trailing_stop_market');
        assert.equal(final.side, 'sell');
        assert.equal(final.price, 96);
        assert.equal(final.amount, 0.03);
        assert.equal(final.remaining, 0.03);
        assert.equal(final.reduceOnly, true);
        assert.equal(final.status, 'open');
        assert.equal(final.trigger, 'last_price');
        const amended = result.account.orders.find((o) => o.id === 'trail-amended');
        assert.equal(amended.amount, 0.02);
        assert.equal(amended.price, 95);
        assert.equal(result.account.hedged, true);
        assert(
          result.account.orders.some((o) => o.id === 'existing-stop' && o.type === 'stop_market' && o.price === 90),
        );
        assert.deepEqual(
          result.account.fills.map(({ id, symbol, side, amount }) => ({ id, symbol, side, amount })),
          [{ id: 'external-filled', symbol, side: 'buy', amount: 0.004 }],
        );
      } finally {
        rmSync(dir, { recursive: true, force: true });
      }
    },
  );
}

test(
  'Classic trailing tape preserves native percentage through an amount edit and cancels by exact native ID',
  { timeout: 45000 },
  () => {
    const dir = mkdtempSync(join(tmpdir(), 'bitget-classic-trailing-'));
    try {
      const tapePath = join(dir, 'private.jsonl');
      const output = join(dir, 'result.json');
      writeFileSync(
        tapePath,
        `${buildBitgetClassicTrailingTape()
          .map((r) => JSON.stringify(r))
          .join('\n')}\n`,
      );
      const run = spawnSync(
        'corepack',
        [
          'yarn',
          '-s',
          'rust:replay',
          '--impl',
          'rust',
          '--venue',
          'bitget',
          '--tape',
          tapePath,
          '--books',
          '',
          '--out',
          output,
        ],
        { cwd: root, encoding: 'utf8', timeout: 40000, env: { ...process.env, ESBUILD_WORKER_THREADS: '0' } },
      );
      assert.ifError(run.error);
      assert.equal(run.status, 0, `${run.stdout}\n${run.stderr}`);
      const result = JSON.parse(readFileSync(output, 'utf8'));
      assert.equal(result.unserved, 0);
      assert.equal(result.unusedWrites, 0);
      assert.equal(result.missingWrites, 0);
      assert.equal(result.invalidSignatures, 0);
      assert.deepEqual(result.protocolErrors, []);
      assert.deepEqual(result.commands, [{ ok: ['classic-created'] }, { ok: ['classic-created'] }, { ok: [] }]);
      const final = result.account.orders.find((o) => o.id === 'classic-final');
      assert(final, JSON.stringify(result.account.orders));
      assert.equal(final.type, 'trailing_stop_market');
      assert.equal(final.price, 97);
      assert.equal(final.amount, 0.03);
      assert.equal(final.side, 'sell');
      assert.equal(final.trigger, 'last_price');
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  },
);
