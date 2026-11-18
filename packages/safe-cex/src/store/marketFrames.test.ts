import type { Market } from '../generated/Market';

import { readFileSync } from 'node:fs';

import { describe, expect, it } from 'vitest';

import { Frame } from '@tealstreet/safe-cex-wasm/wire';
import { nativeFixture } from '../__tests__/cutover/nativeFixture';
import { classifyMarket } from '../instrument-class/resolve';
import { FrameStore } from './frameStore';
import { readMarketFrame } from './marketFrames';

const fixture = () => {
  const bytes = readFileSync('../../sdk-rs/engine/tests/fixtures/market-output.scxf');
  return new Frame(bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength));
};

const readRows = (frame: Frame): Market[] => {
  const rows = readMarketFrame(frame);
  if (rows === undefined) throw new Error('Fixture is missing Market output');
  return rows;
};

describe('native Market output through the real packed decoder', () => {
  it('retains native identity, zero limits, precision and optional metadata through FrameStore serialization', () => {
    const rows = readRows(fixture());
    const store = new FrameStore();
    store.apply(fixture());
    const serialized = JSON.parse(JSON.stringify(store.data)).markets;
    expect(serialized).toEqual(JSON.parse(JSON.stringify(rows)));
    expect(store.marketsById['native-spot']).toBe(store.markets[0]);
    expect(store.marketsBySymbol.BTCUSDT).toBe(store.markets[0]);
    expect(rows[0]).toMatchObject({
      id: 'native-spot',
      symbol: 'BTCUSDT',
      precision: { amount: 0.001, price: 0.1 },
      limits: { amount: { min: 0, max: 0 }, leverage: { min: 0, max: 0 }, price: { min: 0 } },
      contractSize: 0,
      takerFee: 0,
      makerFee: 0,
    });
    expect(serialized[0]).not.toHaveProperty('option');
    expect(serialized[1]).toMatchObject({ id: 'dated-native', prettySymbol: 'BTC/USDT XPERP', isExpiring: true });
    expect(serialized[1]).not.toHaveProperty('contractSize');
    expect(serialized[1]).not.toHaveProperty('takerFee');
    expect(serialized[1].limits).not.toHaveProperty('price');
    expect(serialized[1].info).not.toHaveProperty('expiry');
    expect(serialized[1]).not.toHaveProperty('option');
  });

  it('keeps published zero option terms, but never invents terms for an incomplete option', () => {
    const rows = readRows(fixture());
    expect(rows[2].option).toEqual({
      underlying: 'BTC',
      expiry: 0,
      strike: 0,
      right: 'call',
      style: 'european',
      settlement: 'cash',
      premiumAsset: 'USDT',
      settleAsset: 'USDT',
    });
    expect(rows[3].info.instrumentKind).toBe('PUT');
    expect(rows[3]).not.toHaveProperty('option');
    expect(JSON.parse(JSON.stringify(rows[3].info))).not.toHaveProperty('strike');
  });

  it('passes native classification to the real instrument consumer and clears metadata on replacement', () => {
    const rows = readRows(fixture());
    expect(rows[0].info).toMatchObject({
      symbolType: 'innovation',
      marketRegion: '',
      hlTokenName: 'UBTC',
      hlTokenFullName: 'Unit Bitcoin',
    });
    expect(classifyMarket('bybit', rows[0])).toMatchObject({ kind: 'classified', assetClass: 'crypto' });
    expect(classifyMarket('bybit', { ...rows[0], info: null }).kind).toBe('unknown');
    expect(classifyMarket('bybit', { ...rows[0], info: { symbolType: null } }).kind).toBe('unknown');
    const store = new FrameStore();
    store.apply(
      nativeFixture(
        { markets: rows },
        store.markets.map((row) => row.symbol),
      ),
    );
    store.apply(
      nativeFixture(
        { markets: [{ ...rows[0], info: undefined }] },
        store.markets.map((row) => row.symbol),
      ),
    );
    expect(store.marketsBySymbol.BTCUSDT.info).toBeUndefined();
    expect(store.marketsBySymbol['BTC-CALL']).toBeUndefined();
  });

  it('refuses invalid optional wire numbers instead of silently repairing them', () => {
    const frame = fixture();
    const markets = frame.markets();
    if (markets === undefined) throw new Error('Fixture is missing Market output');
    markets.contractSize[0] = Number.POSITIVE_INFINITY;
    expect(() => readMarketFrame(frame)).toThrow('Nonfinite Market output');
  });
});
