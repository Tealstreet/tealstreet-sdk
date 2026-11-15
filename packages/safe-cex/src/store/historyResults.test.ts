// @vitest-environment node
import type { ClosedPnlEntry } from '../generated/ClosedPnlEntry';
import type { HistoricalOrder } from '../generated/HistoricalOrder';
import type { HistoricalTrade } from '../generated/HistoricalTrade';
import type { IncomeRecord } from '../generated/IncomeRecord';

import { readFileSync } from 'node:fs';

import { describe, expect, it } from 'vitest';

import { EngineHost } from '../../../../sdk-rs/hosts/wasm/js/host.mjs';
import { paginateIncomeRange } from '../exchanges/income-paging';
import { resolveClosedPositions } from '../utils/resolveClosedPositions';

function nativeOutput(): {
  orders: HistoricalOrder[];
  trades: HistoricalTrade[];
  closed: ClosedPnlEntry[];
  income: IncomeRecord[];
} {
  return JSON.parse(
    readFileSync(
      new URL('../../../../sdk-rs/safe-cex-types/tests/fixtures/history-output.json', import.meta.url),
      'utf8',
    ),
  );
}

function reportedNativeOutput(): Record<'bybit' | 'okx' | 'blofin' | 'bitunix', ClosedPnlEntry[]> {
  return JSON.parse(
    readFileSync(
      new URL('../../../../sdk-rs/engine/tests/fixtures/history-closed-output.json', import.meta.url),
      'utf8',
    ),
  );
}

describe('Rust-generated history command results', () => {
  it('decodes actual Rust output through EngineHost without fabricating optional fields or identities', async () => {
    const output = { ...nativeOutput(), closed: Object.values(reportedNativeOutput()).flat() };
    const host = new EngineHost(
      // @ts-expect-error This protocol fixture implements command replies, not the WASM lifecycle.
      {
        command(text: string) {
          const request: unknown = JSON.parse(text);
          if (typeof request !== 'object' || request === null || !('command' in request) || !('op' in request))
            throw new Error('invalid history command');
          const { command, op } = request;
          if (typeof command !== 'number' || (op !== 'orders' && op !== 'trades' && op !== 'closed' && op !== 'income'))
            throw new Error('invalid history operation');
          return JSON.stringify([{ op: 'done', command, result: JSON.stringify(output[op]) }]);
        },
      },
      {
        now: () => 0,
        setTimeout: () => {
          throw new Error('synchronous result');
        },
        clearTimeout: () => {},
      },
    );
    try {
      const orders = await host.command({ op: 'orders' });
      const trades = await host.command({ op: 'trades' });
      expect(await host.command({ op: 'closed' })).toEqual(output.closed);
      if (!Array.isArray(orders) || !Array.isArray(trades)) throw new Error('history result must be an array');
      expect(orders).toEqual(output.orders);
      expect(trades).toEqual(output.trades);
      expect(orders[0]).not.toHaveProperty('price');
      expect(orders[1].price).toBe(0);
      expect(orders[1].realizedPnl).toBe(12);
      expect(orders[1].fees).toBe(2);
      expect(orders[1].rebate).toBe(0.5);
      expect(trades[0]).not.toHaveProperty('id');
      expect(trades[1].id).toBe('execution-2');
      expect(trades[1].orderId).toBe('order-1');
      expect(trades[1].positionSide).toBe('both');
      expect(JSON.parse(JSON.stringify(trades))).toEqual(output.trades);
    } finally {
      host.dispose();
    }
  });

  it('consumes the four native venue reply projections without a TypeScript mapper', async () => {
    for (const entries of Object.values(reportedNativeOutput())) {
      const result = await resolveClosedPositions({ fetchClosedPnl: async () => entries }, { symbol: 'BTC' }, false);
      expect(result.entries).toEqual(entries);
      expect(result.independentlyVerifiable).toBe(true);
      expect(result.entries.every((entry) => entry.provenance === 'venue')).toBe(true);
      expect(result.entries.every((entry) => typeof entry.contracts === 'number')).toBe(true);
    }
  });

  it('keeps numeric strings and venue/derived provenance through the real closed-position consumer', async () => {
    const entries = nativeOutput().closed;
    const result = await resolveClosedPositions({ fetchClosedPnl: async () => entries }, { symbol: 'BTC' }, false);
    expect(result.entries).toEqual(entries);
    expect(result.independentlyVerifiable).toBe(true);
    expect(typeof result.entries[0].fees).toBe('number');
    expect(typeof result.entries[1].fees).toBe('string');
    expect(result.entries[1].provenance).toBe('derived');
    const derived = await resolveClosedPositions(
      {
        fetchClosedPnl: async () => [],
        deriveClosedPositions: async () => entries,
      },
      { symbol: 'BTC' },
      true,
    );
    expect(derived.entries.every((entry) => entry.provenance === 'derived')).toBe(true);
    expect(derived.independentlyVerifiable).toBe(false);
  });

  it('preserves native nulls and measured zero through host, worker JSON and resolver', async () => {
    const rows: ClosedPnlEntry[] = JSON.parse(
      readFileSync(
        new URL('../../../../sdk-rs/engine/tests/fixtures/history-unknown-output.json', import.meta.url),
        'utf8',
      ),
    );
    const host = new EngineHost(
      // @ts-expect-error A synchronous native command reply fixture supplies no lifecycle.
      {
        command(text: string) {
          const { command } = JSON.parse(text);
          return JSON.stringify([{ op: 'done', command, result: JSON.stringify(rows) }]);
        },
      },
      {
        now: () => 0,
        setTimeout: () => {
          throw new Error('synchronous');
        },
        clearTimeout: () => {},
      },
    );
    try {
      const result = await resolveClosedPositions(
        { fetchClosedPnl: async () => JSON.parse(JSON.stringify(await host.command({ op: 'fetchClosedPnl' }))) },
        { symbol: 'BTCUSDT' },
        false,
      );
      expect(result.entries).toHaveLength(2);
      for (const field of [
        'notional',
        'contracts',
        'entryPrice',
        'exitPrice',
        'unrealizedPnl',
        'fees',
        'leverage',
        'time',
        'closedTime',
      ] as const) {
        expect(result.entries[0][field]).toBe(0);
        expect(result.entries[1][field]).toBeNull();
      }
    } finally {
      host.dispose();
    }
  });

  it('retains signed fees, funding and gross PnL as separate income records sharing a native bill ID', async () => {
    const income = nativeOutput().income;
    let served = false;
    const result = await paginateIncomeRange(
      async () => {
        if (served) return [];
        served = true;
        return income;
      },
      { startTime: 0, endTime: 10, chunkMs: 10, pageLimit: 100 },
    );
    expect(result.truncated).toBe(false);
    expect(result.records).toHaveLength(income.length);
    expect(result.records.find((row) => row.category === 'realized_pnl')?.income).toBe(12);
    expect(result.records.find((row) => row.category === 'commission')?.income).toBe(-2);
    expect(result.records.find((row) => row.category === 'funding')?.income).toBe(-1);
    expect(result.records.find((row) => row.category === 'transfer')?.income).toBe(0);
    expect(result.records.every((row) => row.tranId === 'bill-1')).toBe(true);
    expect(result.records[0]).not.toHaveProperty('tradeId');
  });
});
