// @vitest-environment node
import type { Execution } from '../generated/Execution';
import type { OrderFillEvent } from '../generated/OrderFillEvent';
import type { PublicTrade } from '../generated/PublicTrade';

import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

import { beforeAll, describe, expect, it } from 'vitest';

import { Frame } from '@tealstreet/safe-cex-wasm/wire';
import { reconstructPositionIntervals } from '../journal/position-intervals';
import { OrderSide, PositionSide } from '../types';
import { readFillFrame } from './fillFrames';
import { FrameApplier } from './frameApplier';
import { DefaultStore } from './store.base';

let native: { frame: number[]; events: OrderFillEvent[] };
const frame = () => new Frame(Uint8Array.from(native.frame).buffer);
const fillRows = (decoded: Frame) => {
  const rows = decoded.fills();
  if (rows === undefined) throw new Error('Native fixture must contain fill rows');
  return rows;
};

beforeAll(() => {
  const manifest = fileURLToPath(new URL('../../../../sdk-rs/Cargo.toml', import.meta.url));
  native = JSON.parse(
    execFileSync(
      'cargo',
      [
        'run',
        '-j2',
        '--locked',
        '--quiet',
        '--manifest-path',
        manifest,
        '-p',
        'safe-cex-engine',
        '--example',
        'fill_contract_fixture',
      ],
      { cwd: fileURLToPath(new URL('../../../../sdk-rs', import.meta.url)), encoding: 'utf8', timeout: 120_000 },
    ),
  );
}, 120_000);

describe('native Fill output through the real packed decoder', () => {
  it('retains execution vs order identity, incremental amount, optional zeroes and native absence', () => {
    const fills: OrderFillEvent[] = readFillFrame(frame());
    expect(fills).toEqual(native.events);
    expect(fills[0]).toMatchObject({ id: 'native-fill-1', orderId: 'order-1', amount: 0.25, fee: 0, realizedPnl: 0 });
    expect(fills[1]).toMatchObject({ id: 'order-1', orderId: 'order-1', amount: 0.75 });
    expect(fills[1]).not.toHaveProperty('fee');
    expect(fills[1]).not.toHaveProperty('realizedPnl');
    expect(fills[2]).not.toHaveProperty('fee');
    expect(fills[2]).not.toHaveProperty('realizedPnl');
    expect(fills.reduce((amount, fill) => amount + fill.amount, 0)).toBe(1);
  });

  it('keeps native identity without market-cache repair and survives Store plus JSON serialization', () => {
    const store = new DefaultStore();
    const applier = new FrameApplier(store);
    const decoded = frame();
    applier.apply(decoded);
    const fills = readFillFrame(decoded);
    expect(applier.fills(decoded)).toEqual(fills);
    expect(store.markets).toEqual([]);
    expect(JSON.parse(JSON.stringify({ store: store.serialize(), fills })).fills).toEqual(native.events);
    expect(fills.map((fill) => fill.symbol)).toEqual(['BTC-PERP', 'BTC-PERP', 'BTC-PERP']);
  });

  it('rejects a missing native event id instead of synthesizing identity in TypeScript', () => {
    const decoded = frame();
    const table = fillRows(decoded);
    const numericEnd = table.rows.byteOffset + table.count * 16;
    const ids = new Uint32Array(decoded.buffer, numericEnd + table.count * 4, table.count);
    ids[0] = 0xffffffff;
    expect(() => readFillFrame(decoded)).toThrow('Fill identity requires a native event id');
  });

  it('rejects invalid side codes instead of casting an incomplete event', () => {
    const decoded = frame();
    fillRows(decoded).side[0] = 7;
    expect(() => readFillFrame(decoded)).toThrow('Invalid Fill side');
  });

  it('rejects malformed infinite financial columns instead of treating them as absent', () => {
    const decoded = frame();
    const fee = fillRows(decoded).fee;
    if (fee === undefined) throw new Error('Native fixture must contain the fee column');
    fee[0] = Infinity;
    expect(() => readFillFrame(decoded)).toThrow('Invalid Fill financial column');
  });
});

describe('consumers of the generated execution declarations', () => {
  it('folds seconds into millisecond intervals without losing a reported zero or subtracting fees twice', () => {
    const rows: Execution[] = [
      {
        id: 'open',
        orderId: 'order-open',
        timestamp: 1000,
        symbol: 'BTC-PERP',
        side: OrderSide.Buy,
        price: 100,
        qty: 1,
        commission: 0,
      },
      {
        id: 'close',
        orderId: 'order-close',
        timestamp: 1001,
        symbol: 'BTC-PERP',
        side: OrderSide.Sell,
        price: 100,
        qty: 1,
        commission: 0.5,
        realizedPnl: 0,
        venueReportedRevenue: 0,
      },
    ];
    const [interval] = reconstructPositionIntervals(rows);
    expect(interval).toMatchObject({ openedAt: 1_000_000, closedAt: 1_001_000, fees: 0.5, realizedPnl: 0 });
    const unreported: Execution[] = rows.map((row) => ({ ...row, realizedPnl: undefined }));
    expect(reconstructPositionIntervals(unreported)[0].realizedPnl).toBeUndefined();
    const hedged: Execution[] = rows.map((row) => ({ ...row, positionSide: PositionSide.Long }));
    expect(reconstructPositionIntervals(hedged)[0].side).toBe(PositionSide.Long);
    expect(JSON.parse(JSON.stringify(rows))[0]).not.toHaveProperty('positionSide');
  });

  it('preserves PublicTrade milliseconds and distinguishes absent quote quantity from zero', () => {
    const trade: PublicTrade = {
      id: 'trade',
      timestamp: 1_700_000_000_125,
      symbol: 'BTC-PERP',
      side: OrderSide.Buy,
      price: 0,
      qty: 0,
    };
    expect(JSON.parse(JSON.stringify(trade))).not.toHaveProperty('quoteQty');
    expect(JSON.parse(JSON.stringify({ ...trade, quoteQty: 0 }))).toMatchObject({
      timestamp: 1_700_000_000_125,
      quoteQty: 0,
    });
  });
});
