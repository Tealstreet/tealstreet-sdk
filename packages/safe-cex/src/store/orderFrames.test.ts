import type { Order } from '../generated/Order';
import type { Market } from '../types';

import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

import { Frame } from '@tealstreet/safe-cex-wasm/wire';
import { formatOrderBaseSize } from '../../../cli-core/src/utils/formatBaseSize';
import { OrderRejectReason } from '../generated/OrderRejectReason';
import { OrderStatus } from '../generated/OrderStatus';
import { OrderStopTriggerType } from '../generated/OrderStopTriggerType';
import { readOrderFrame } from './orderFrames';
import { DefaultStore } from './store.base';

const sdk = resolve(__dirname, '../../../../sdk-rs');
const frames: number[][] = JSON.parse(
  process.env.ORDER_OUTPUT_FIXTURE
    ? readFileSync(process.env.ORDER_OUTPUT_FIXTURE, 'utf8')
    : execFileSync(
        'cargo',
        [
          'run',
          '-j',
          '2',
          '--locked',
          '-p',
          'safe-cex-engine',
          '--manifest-path',
          resolve(sdk, 'Cargo.toml'),
          '--example',
          'order_output_fixture',
          '--quiet',
        ],
        { env: { ...process.env, CARGO_BUILD_JOBS: '2' }, maxBuffer: 4 * 1024 * 1024 },
      ).toString(),
);
const frame = (bytes: number[]) => new Frame(Uint8Array.from(bytes).buffer);

function required<T>(value: T | undefined | null, label: string): T {
  if (value === undefined || value === null) throw new Error(`Expected ${label}`);
  return value;
}

const orders = (input: Frame) => required(input.orders(), 'native order table');
const decodedOrders = (input: Frame, store: DefaultStore) => required(readOrderFrame(input, store), 'decoded orders');
const profiles: [number, number[]][] = frames.slice(0, 7).map((bytes, index) => [index, bytes]);

describe('Rust Order output through the packed frame and Store consumer', () => {
  it('materializes placement units in Rust before the JS decoder sees them', () => {
    const nativePlacement = orders(frame(frames[1]));
    expect(required(nativePlacement.remainingNative, 'remainingNative column')[0]).toBe(7);
    expect(required(nativePlacement.filledInPlacementUnits, 'filledInPlacementUnits column')[0]).toBe(3);
    const contractCount = orders(frame(frames[4]));
    expect(required(contractCount.remainingNative, 'remainingNative column')[0]).toBe(7);
    expect(required(contractCount.filledInPlacementUnits, 'filledInPlacementUnits column')[0]).toBe(3);
    expect(required(orders(frame(frames[2])).remainingNotional, 'remainingNotional column')[1]).toBe(700);
  });

  it('does not recreate absent native quantities from base sizes', () => {
    const input = frame(frames[4]);
    const table = orders(input);
    required(table.remainingNative, 'remainingNative column')[0] = NaN;
    required(table.filledInPlacementUnits, 'filledInPlacementUnits column')[0] = NaN;
    const row = decodedOrders(input, new DefaultStore())[0];
    expect(row.extra).toBeUndefined();
    expect(row.remaining).toBe(0.7);
    expect(row.filled).toBe(0.3);
  });

  it('refuses missing required enums and malformed optional numbers', () => {
    const missing = frame(frames[0]);
    orders(missing).enums[0] |= 255;
    expect(() => readOrderFrame(missing, new DefaultStore())).toThrow('missing Order enum');
    const malformed = frame(frames[0]);
    required(orders(malformed).remainingNative, 'remainingNative column')[0] = Infinity;
    expect(() => readOrderFrame(malformed, new DefaultStore())).toThrow('invalid Order optional number');
  });

  it.each(profiles)(
    'preserves exact extras, absent/zero triggers and terminal identity for profile %i',
    (_, bytes) => {
      const store = new DefaultStore();
      const rows = decodedOrders(frame(bytes), store);
      const [ordinary, exact, terminal] = rows;
      const ordinaryExtras = [
        undefined,
        { remainingNative: 7, filledInPlacementUnits: 3 },
        { remainingNative: 7 },
        { remainingNative: 9.7 },
        { remainingNative: 7, filledInPlacementUnits: 3 },
        undefined,
        undefined,
      ];
      expect(ordinary.extra).toEqual(ordinaryExtras[_]);
      expect(ordinary).not.toHaveProperty('takeProfit');
      expect(ordinary).not.toHaveProperty('stopLoss');
      expect(ordinary).not.toHaveProperty('close');
      expect(exact).toMatchObject({
        id: 'exact',
        parentId: 'parent',
        clientOrderId: 'client',
        close: false,
        takeProfit: 0,
        stopLoss: 0,
        triggerType: OrderStopTriggerType.MarkPrice,
        slTriggerType: OrderStopTriggerType.MarkPrice,
        extra: {
          remainingNative: 7,
          filledInPlacementUnits: 3,
          remainingNotional: 700,
          conditionalOrderPrice: 0,
          rejectReason: OrderRejectReason.PostOnly,
        },
      });
      expect(terminal.status).toBe(OrderStatus.Canceled);
      expect(rows.map((order) => [order.amount, order.filled, order.remaining])).toEqual([
        [10, 0.3, 0.7],
        [10, 0.3, 0.7],
        [10, 0.3, 0.7],
      ]);
      store.update({ orders: rows });
      expect(JSON.parse(JSON.stringify(store.serialize())).orders).toEqual(rows);
      const buffer = Uint8Array.from(bytes).buffer;
      const transferred = structuredClone(buffer, { transfer: [buffer] });
      expect(buffer.byteLength).toBe(0);
      expect(readOrderFrame(new Frame(transferred), new DefaultStore())).toEqual(rows);
    },
  );

  it('preserves explicit inverse-native remaining without market metadata', () => {
    const rows = readOrderFrame(frame(frames[7]), new DefaultStore());
    if (!rows) throw new Error('Expected native order rows');
    expect(rows[1].extra?.remainingNative).toBe(7);
    expect(rows[0].extra?.remainingNative).toBe(9.7);
  });

  it('feeds generated orders into real mutation consumers without losing exact leaves or zero stops', () => {
    const store = new DefaultStore();
    const rows = decodedOrders(frame(frames[4]), store);
    store.addOrUpdateOrders(rows);
    store.updateOrder(rows[1], { stopLoss: 0 });
    const result: Order | undefined = store.orders.find((order) => order.id === 'exact');
    expect(result?.extra?.remainingNative).toBe(7);
    expect(result?.stopLoss).toBe(0);
    expect(result?.parentId).toBe('parent');
    // CLI must use the reported inverse notional instead of the base remainder.
    const market: Market = {
      id: 'BTC',
      symbol: 'BTC',
      base: 'BTC',
      quote: 'USD',
      active: true,
      isInverse: true,
      precision: { amount: 1, price: 1 },
      limits: { amount: { min: 1, max: 100 }, leverage: { min: 1, max: 10 } },
    };
    expect(formatOrderBaseSize({ ...rows[1], price: 100 }, market)).toBe('7.00');
    expect(Object.values(OrderStatus)).toEqual(['open', 'closed', 'canceled', 'creating', 'updating', 'canceling']);
  });
});
