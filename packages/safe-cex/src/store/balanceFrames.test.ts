import { execFileSync } from 'node:child_process';
import { resolve } from 'node:path';
import { beforeAll, describe, expect, it } from 'vitest';
import { Frame } from '@tealstreet/safe-cex-wasm/wire';
import type { Balance } from '../generated/Balance';
import type { Market } from '../types';
import { emptyBalance } from '../generated/Balance';
import { getBalanceSlice } from '../utils/getBalanceSlice';
import { getTotalBalanceFromBalance } from '../utils/getTotalBalanceFromBalance';
import { readBalanceFrame } from './balanceFrames';
import { DefaultStore } from './store.base';

type Fixture = { name: string; bytes: number[] };
let fixtures: Fixture[];
function frame(name: string): Frame {
  const bytes = Uint8Array.from(fixtures.find(f => f.name === name)!.bytes);
  return new Frame(bytes.buffer);
}
beforeAll(() => {
  fixtures = JSON.parse(execFileSync('cargo', ['run', '--locked', '-j', '2', '-q', '-p', 'safe-cex-engine', '--example', 'balance_contract_frames'], {
    cwd: resolve(__dirname, '../../../../sdk-rs'), encoding: 'utf8',
  }));
}, 60_000);

describe('native Balance frames into the generated contract', () => {
  it('keeps an absent section absent and an explicit zero balance present', () => {
    expect(readBalanceFrame(frame('absent'))).toBeUndefined();
    expect(readBalanceFrame(frame('zero'))).toEqual({ used: 0, free: 0, total: 0, upnl: 0 });
    const before = frame('unready');
    expect(readBalanceFrame(before)).toEqual(emptyBalance);
    expect(before.status()!.loaded & 1).toBe(0);
    expect(frame('zero').status()!.loaded & 1).toBe(1);
  });
  it('decodes distinct native aggregate and dictionary columns without coercion', () => {
    const balance = readBalanceFrame(frame('rich'))!;
    expect(balance).toMatchObject({ used: 11.25, free: 22.5, total: 33.75, upnl: -4.5 });
    expect(Object.keys(balance.coins!)).toEqual(['2', '10', 'BTC', '__proto__']);
    expect(balance.coins!.__proto__).toEqual({ base: '__proto__', amount: 7, notional: 8 });
    expect(balance.coins!.BTC).toEqual({ base: 'BTC', amount: 0, notional: 0 });
    expect(Object.keys(balance.currencies!)).toEqual(['2', '10', 'USD', '__proto__']);
    expect(balance.currencies!.USD).toEqual({ used: 13, free: 17, total: 19, upnl: -23 });
    balance.coins!.BTC.amount = 3;
    expect(balance.coins!.BTC.amount).toBe(3);
  });
  it('replaces authoritative optional dictionaries through DefaultStore and serialization', () => {
    const store = new DefaultStore();
    const rich = readBalanceFrame(frame('rich'))!;
    store.update({ balance: rich });
    const snapshot = store.serialize();
    expect(snapshot.balance).toEqual(rich);
    expect(JSON.parse(JSON.stringify(snapshot)).balance.coins.__proto__.amount).toBe(7);
    const empty = readBalanceFrame(frame('empty-maps'))!;
    store.update({ balance: empty });
    expect(store.balance.coins).toEqual({});
    expect(store.balance.currencies).toEqual({});
    store.update({ balance: readBalanceFrame(frame('zero'))! });
    expect(store.balance).not.toHaveProperty('coins');
    expect(store.balance).not.toHaveProperty('currencies');
    expect(snapshot.balance.coins!.BTC.amount).toBe(0);
  });
  it('keeps omitted coins distinct from an explicitly empty collateral map', () => {
    expect(readBalanceFrame(frame('currencies-only'))).not.toHaveProperty('coins');
    expect(readBalanceFrame(frame('empty-coins'))).toHaveProperty('coins', {});
    expect(readBalanceFrame(frame('empty-coins'))).not.toHaveProperty('currencies');
  });
  it('retains native nonfinite numbers and signed zero rather than fabricating display defaults', () => {
    const balance = readBalanceFrame(frame('nonfinite'))!;
    expect(Number.isNaN(balance.used)).toBe(true);
    expect(balance.free).toBe(Infinity);
    expect(balance.total).toBe(-Infinity);
    expect(Object.is(balance.upnl, -0)).toBe(true);
  });
  it('rejects truncated frames without mutating a previously decoded balance', () => {
    const good = readBalanceFrame(frame('rich'))!;
    const bytes = Uint8Array.from(fixtures.find(f => f.name === 'rich')!.bytes);
    expect(() => readBalanceFrame(new Frame(bytes.buffer.slice(0, -8)))).toThrow();
    expect(good.free).toBe(22.5);
  });
  it('real balance consumers preserve zero, omitted maps and independent currency values', () => {
    const balance: Balance = readBalanceFrame(frame('rich'))!;
    expect(getBalanceSlice(undefined)).toBeNull();
    expect(getBalanceSlice(balance)).toBe(balance);
    expect(getTotalBalanceFromBalance(balance)).toBe(33.75);
    expect(getTotalBalanceFromBalance(readBalanceFrame(frame('zero')))).toBe(0);
    const market: Market = { id: 'BTCUSD', symbol: 'BTC/USD', base: 'BTC', quote: 'USD', active: true,
      precision: { amount: 0.001, price: 0.01 }, limits: { amount: { min: 0, max: 100 }, leverage: { min: 1, max: 20 } } };
    expect(getBalanceSlice(balance, market)).toBe(balance.currencies!.USD);
    expect(getTotalBalanceFromBalance(balance, market)).toBe(19);
  });
});
