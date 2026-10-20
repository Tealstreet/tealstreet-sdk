import { describe, expect, it } from 'vitest';
import { emptyTxFlowTicker, mapTxFlowCandle, mapTxFlowContext, mapTxFlowMarket, mapTxFlowTicker } from '../reference/txflow/txflow.public';
import type { TxFlowInstrument } from '../reference/txflow/txflow.types';

const instrument: TxFlowInstrument = { index: 1, name: 'BTC-USDC', baseCurrency: 'BTC', quoteCurrency: 'USDC', currencyType: 'usdc', basePrecision: '0.0001', priceTick: '0.1', maxLeverage: 50, haltTrading: false, delisted: false, onlyIsolated: false, useNotionalMargin: false };
describe('TxFlow public native payload semantics', () => {
  it('uses explicit numeric identity, exact ticks and raw metadata', () => {
    const market = mapTxFlowMarket(instrument)!;
    expect(market).toMatchObject({ id: '1', symbol: 'BTC-USDC', precision: { price: 0.1, amount: 0.0001 }, contractSize: 1 });
    expect(market.info).toBe(instrument);
    expect(mapTxFlowMarket({ ...instrument, currencyType: 'btc' })).toBeUndefined();
    expect(mapTxFlowMarket({ ...instrument, delisted: true })).toBeUndefined();
    expect(mapTxFlowMarket({ ...instrument, haltTrading: true })?.active).toBe(false);
    expect(mapTxFlowMarket({ ...instrument, basePrecision: '100' })?.precision.amount).toBe(100);
  });
  it('converts funding percent and OI base units, preserving last when node has no trade', () => {
    const ticker = emptyTxFlowTicker(mapTxFlowMarket(instrument)!);
    const mapped = mapTxFlowContext({ ...ticker, last: 65000 }, { funding: '-0.01', markPx: '65001', oraclePx: '65000', openInterest: '2', midPx: '65000.5', nextFundingTime: 7200000 });
    expect(mapped).toMatchObject({ last: 65000, mark: 65001, index: 65000, openInterest: 130002, fundingRate: -0.0001, fundingTime: 7200000, midPrice: 65000.5 });
  });
  it('keeps API last/volume separate from mark and clears empty full snapshots', () => {
    const ticker = { ...emptyTxFlowTicker(mapTxFlowMarket(instrument)!), mark: 65000, fundingRate: 0.0001 };
    const live = mapTxFlowTicker(ticker, { instrumentId: 1, lastPrice: '65020', prevDayPx: '65000', coinVlm: '2', dayNtlVlm: '130040' });
    expect(live).toMatchObject({ last: 65020, mark: 65000, fundingRate: 0.0001, volume: 2, quoteVolume: 130040 });
    expect(mapTxFlowTicker(live, { instrumentId: 1, lastPrice: '', coinVlm: '', dayNtlVlm: '' })).toMatchObject({ last: 0, volume: 0, quoteVolume: 0 });
  });
  it('rejects ticker bodies routed to the wrong instrument', () => {
    const ticker = emptyTxFlowTicker(mapTxFlowMarket(instrument)!);
    expect(() => mapTxFlowTicker(ticker, { instrumentId: 2, lastPrice: '100' }, true)).toThrow('Mismatched TxFlow ticker');
  });
  it('converts documented candle millisecond timestamps and rejects nonfinite values', () => {
    expect(mapTxFlowCandle({ t: 60000, s: '1', i: '1m', o: '2', h: '3', l: '1', c: '2.5', v: '4' })).toEqual({ timestamp: 60, open: 2, high: 3, low: 1, close: 2.5, volume: 4 });
    expect(() => mapTxFlowMarket({ ...instrument, priceTick: 'NaN' })).toThrow('Invalid TxFlow numeric');
  });
});
