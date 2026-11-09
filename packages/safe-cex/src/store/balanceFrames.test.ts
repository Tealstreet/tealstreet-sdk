import assert from 'node:assert/strict';
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
import { FrameApplier } from './frameApplier';
import { DefaultStore } from './store.base';

type Fixture = { name: string; bytes: number[] };
let fixtures: Fixture[];
function frame(name: string): Frame {
  const fixture = fixtures.find(f => f.name === name);
  assert.ok(fixture, `missing native fixture ${name}`);
  const bytes = Uint8Array.from(fixture.bytes);
  return new Frame(bytes.buffer);
}
function isFixture(value: unknown): value is Fixture {
  return typeof value === 'object' && value !== null && 'name' in value && typeof value.name === 'string'
    && 'bytes' in value && Array.isArray(value.bytes)
    && value.bytes.every((byte: unknown) => typeof byte === 'number' && Number.isInteger(byte) && byte >= 0 && byte <= 255);
}
beforeAll(() => {
  const parsed: unknown = JSON.parse(execFileSync('cargo', ['run', '--locked', '-j', '2', '-q', '-p', 'safe-cex-engine', '--example', 'balance_contract_frames'], {
    cwd: resolve(__dirname, '../../../../sdk-rs'), encoding: 'utf8',
  }));
  assert.ok(Array.isArray(parsed));
  const rows: Fixture[] = [];
  for (const value of parsed) {
    assert.ok(isFixture(value));
    rows.push(value);
  }
  fixtures = rows;
}, 60_000);
function requiredBalance(name: string): Balance {
  const balance = readBalanceFrame(frame(name));
  assert.ok(balance, `missing native balance ${name}`);
  return balance;
}


describe('native Balance frames into the generated contract', () => {
  it('keeps an absent section absent and an explicit zero balance present', () => {
    expect(readBalanceFrame(frame('absent'))).toBeUndefined();
    expect(readBalanceFrame(frame('zero'))).toEqual({ used: 0, free: 0, total: 0, upnl: 0 });
    const before = frame('unready');
    expect(readBalanceFrame(before)).toEqual(emptyBalance);
    const unready = before.status();
    const ready = frame('zero').status();
    assert.ok(unready);
    assert.ok(ready);
    expect(unready.loaded & 1).toBe(0);
    expect(ready.loaded & 1).toBe(1);
  });
  it('decodes distinct native aggregate and dictionary columns without coercion', () => {
    const balance = requiredBalance('rich');
    expect(balance).toMatchObject({ used: 11.25, free: 22.5, total: 33.75, upnl: -4.5 });
    assert.ok(balance.coins);
    assert.ok(balance.currencies);
    expect(Object.keys(balance.coins)).toEqual(['2', '10', 'BTC', '__proto__']);
    expect(balance.coins.__proto__).toEqual({ base: '__proto__', amount: 7, notional: 8 });
    expect(balance.coins.BTC).toEqual({ base: 'BTC', amount: 0, notional: 0 });
    expect(Object.keys(balance.currencies)).toEqual(['2', '10', 'USD', '__proto__']);
    expect(balance.currencies.USD).toEqual({ used: 13, free: 17, total: 19, upnl: -23 });
    balance.coins.BTC.amount = 3;
    expect(balance.coins.BTC.amount).toBe(3);
  });
  it('replaces authoritative optional dictionaries through DefaultStore and serialization', () => {
    const store = new DefaultStore();
    const rich = requiredBalance('rich');
    store.update({ balance: rich });
    const snapshot = store.serialize();
    assert.ok(snapshot.balance.coins);
    expect(snapshot.balance).toEqual(rich);
    const serialized: unknown = JSON.parse(JSON.stringify(snapshot));
    assert.ok(serialized && typeof serialized === 'object' && 'balance' in serialized);
    assert.deepEqual(serialized.balance, snapshot.balance);
    const empty = requiredBalance('empty-maps');
    store.update({ balance: empty });
    expect(store.balance.coins).toEqual({});
    expect(store.balance.currencies).toEqual({});
    store.update({ balance: requiredBalance('zero') });
    expect(store.balance).not.toHaveProperty('coins');
    expect(store.balance).not.toHaveProperty('currencies');
    expect(snapshot.balance.coins.BTC.amount).toBe(0);
  });
  it('keeps omitted coins distinct from an explicitly empty collateral map', () => {
    expect(readBalanceFrame(frame('currencies-only'))).not.toHaveProperty('coins');
    expect(readBalanceFrame(frame('empty-coins'))).toHaveProperty('coins', {});
    expect(readBalanceFrame(frame('empty-coins'))).not.toHaveProperty('currencies');
  });
  it.each(['zero', 'currencies-only'])('merges published balance fields and retains omitted maps through FrameApplier with %s', (next) => {
    const store = new DefaultStore();
    const applier = new FrameApplier(store);
    applier.apply(frame('rich'));
    const previous = store.serialize().balance;
    assert.ok(previous.coins);
    assert.ok(previous.currencies);
    expect(previous.coins.BTC.amount).toBe(0);

    applier.apply(frame(next));
    const expected = { ...previous, ...requiredBalance(next) };
    expect(store.balance).toEqual(expected);
    expect(store.balance.coins).toEqual(previous.coins);
    expect(store.serialize().balance).toEqual(expected);
    expect(store.balance.currencies).toEqual(expected.currencies);
    expect(previous.coins.BTC.amount).toBe(0);
    expect(previous.currencies.USD.total).toBe(19);
  });
  it.each(['zero', 'currencies-only'])('preserves separately fetched info and omitted maps with %s', (next) => {
    const store = new DefaultStore();
    const info = { accountType: 'UNIFIED', collateral: { BTC: true }, tiers: [{ tier: 1 }] };
    store.update({ balance: { ...emptyBalance, info } });
    const applier = new FrameApplier(store);
    applier.apply(frame('rich'));
    expect(store.balance.info).toBe(info);
    assert.ok(store.balance.coins);
    assert.ok(store.balance.currencies);

    const previous = store.serialize().balance;
    applier.apply(frame(next));
    const expected = { ...previous, ...requiredBalance(next) };
    expect(store.balance.info).toBe(info);
    expect(store.balance).toEqual(expected);
    expect(store.balance.coins).toEqual(previous.coins);
    expect(store.serialize().balance).toEqual(expected);
    expect(store.balance.currencies).toEqual(expected.currencies);
  });
  it('retains native nonfinite numbers and signed zero rather than fabricating display defaults', () => {
    const balance = requiredBalance('nonfinite');
    expect(Number.isNaN(balance.used)).toBe(true);
    expect(balance.free).toBe(Infinity);
    expect(balance.total).toBe(-Infinity);
    expect(Object.is(balance.upnl, -0)).toBe(true);
  });
  it('rejects truncated frames without mutating a previously decoded balance', () => {
    const good = requiredBalance('rich');
    const fixture = fixtures.find(f => f.name === 'rich');
    assert.ok(fixture);
    const bytes = Uint8Array.from(fixture.bytes);
    expect(() => readBalanceFrame(new Frame(bytes.buffer.slice(0, -8)))).toThrow();
    expect(good.free).toBe(22.5);
  });
  it('real balance consumers preserve zero, omitted maps and independent currency values', () => {
    const balance: Balance = requiredBalance('rich');
    expect(getBalanceSlice(undefined)).toBeNull();
    expect(getBalanceSlice(balance)).toBe(balance);
    expect(getTotalBalanceFromBalance(balance)).toBe(33.75);
    expect(getTotalBalanceFromBalance(readBalanceFrame(frame('zero')))).toBe(0);
    assert.ok(balance.currencies);
    const market: Market = { id: 'BTCUSD', symbol: 'BTC/USD', base: 'BTC', quote: 'USD', active: true,
      precision: { amount: 0.001, price: 0.01 }, limits: { amount: { min: 0, max: 100 }, leverage: { min: 1, max: 20 } } };
    expect(getBalanceSlice(balance, market)).toBe(balance.currencies.USD);
    expect(getTotalBalanceFromBalance(balance, market)).toBe(19);
  });
});
