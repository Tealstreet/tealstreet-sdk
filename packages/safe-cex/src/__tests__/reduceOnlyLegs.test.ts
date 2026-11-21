import type { PlaceOrderOpts } from '../types';

import { describe, expect, it, vi } from 'vitest';

import { FrameStore } from '../store/frameStore';
import { OrderSide, OrderType } from '../types';
import { ApexOmniExchange } from './reference/apexomni/apexomni.exchange';
import { AsterDEXExchange } from './reference/asterdex/asterdex.exchange';
import { BackpackExchange } from './reference/backpack/backpack.exchange';
import { BinanceExchange } from './reference/binance/binance.exchange';
import { BinanceInverseExchange } from './reference/binance/binanceinverse.exchange';
import { BinancePMExchange } from './reference/binance/binancepm.exchange';
import { BinanceSpotExchange } from './reference/binance/binancespot.exchange';
import { BingxExchange } from './reference/bingx/bingx.exchange';
import { BitgetExchange } from './reference/bitget/bitget.exchange';
import { BitrueExchange } from './reference/bitrue/bitrue.exchange';
import { BitunixExchange } from './reference/bitunix/bitunix.exchange';
import { BlofinExchange } from './reference/blofin/blofin.exchange';
import { BybitExchange } from './reference/bybit/bybit.exchange';
import { BybitInverseExchange } from './reference/bybit/bybitinverse.exchange';
import { BybitSpotExchange } from './reference/bybit/bybitspot.exchange';
import { CoinbaseExchange } from './reference/coinbase/coinbase.exchange';
import { CryptocomExchange } from './reference/cryptocom/cryptocom.exchange';
import { GrvtExchange } from './reference/grvt/grvt.exchange';
import { HyperliquidExchange } from './reference/hyperliquid/hyperliquid.exchange';
import { HyperliquidSpotExchange } from './reference/hyperliquid/hyperliquidspot.exchange';
import { KrakenExchange } from './reference/kraken/kraken.exchange';
import { KrakenSpotExchange } from './reference/kraken/krakenspot.exchange';
import { KucoinExchange } from './reference/kucoin/kucoin.exchange';
import { LighterExchange } from './reference/lighter/lighter.exchange';
import { MexcExchange } from './reference/mexc/mexc.exchange';
import { OKXExchange } from './reference/okx/okx.exchange';
import { OrderlyExchange } from './reference/orderly/orderly.exchange';
import { ParadexExchange } from './reference/paradex/paradex.exchange';
import { PhemexExchange } from './reference/phemex/phemex.exchange';
import { REDUCE_ONLY_LEGS_REFUSAL, reduceOnlyWithAttachedLegs } from './reference/stopEntryLegs';
import { WeexExchange } from './reference/weex/weex.exchange';
import { WOOXExchange } from './reference/woo/woo.exchange';

// AGT-120: an order that can only reduce (reduceOnly or close) never carries legs; they would
// outlive the position as stray reduce-only triggers. Mirrors the Rust engine's gate.
describe('reduceOnlyWithAttachedLegs', () => {
  const order = (extra: Partial<PlaceOrderOpts>) =>
    ({
      symbol: 'BTCUSDT',
      type: OrderType.Limit,
      side: OrderSide.Sell,
      amount: 1,
      price: 100,
      ...extra,
    }) as PlaceOrderOpts;

  it('flags a reduce-only or close order with a non-zero leg, and nothing else', () => {
    for (const type of [OrderType.Limit, OrderType.Market, OrderType.StopLoss]) {
      expect(reduceOnlyWithAttachedLegs(order({ type, reduceOnly: true, stopLoss: 120 }))).toBe(true);
      expect(reduceOnlyWithAttachedLegs(order({ type, close: true, takeProfit: 80 }))).toBe(true);
      expect(reduceOnlyWithAttachedLegs(order({ type, reduceOnly: true, stopLoss: 0, takeProfit: 80 }))).toBe(true);
      expect(reduceOnlyWithAttachedLegs(order({ type, reduceOnly: true }))).toBe(false);
      expect(reduceOnlyWithAttachedLegs(order({ type, reduceOnly: true, stopLoss: 0, takeProfit: 0 }))).toBe(false);
      expect(reduceOnlyWithAttachedLegs(order({ type, reduceOnly: false, close: false, stopLoss: 120 }))).toBe(false);
      expect(reduceOnlyWithAttachedLegs(order({ type, stopLoss: 120, takeProfit: 80 }))).toBe(false);
    }
  });

  it('uses the engine copy, leaving "not sent" to the outcome and the app toast', () => {
    expect(REDUCE_ONLY_LEGS_REFUSAL).toBe(
      "A stop-loss or take-profit can't be attached to a reduce-only order. Place it as its own order.",
    );
  });
});

describe('every reference venue refuses a reduce-only order with legs before any write', () => {
  const options = {
    key: 'test-key',
    secret: '0x' + '11'.repeat(32),
    passphrase: 'test-passphrase',
    applicationId: '0x' + '22'.repeat(20),
    apiKeyIndex: '2',
    tuning: { consoleLogging: false },
  };
  const store = () => new FrameStore() as any;
  const subjects: [string, () => any][] = [
    ['apexomni', () => new ApexOmniExchange('apexomni' as any, options as any, store())],
    ['asterdex', () => new AsterDEXExchange('asterdex' as any, options as any, store())],
    ['backpack', () => new BackpackExchange('backpack' as any, options as any, store())],
    ['binance', () => new BinanceExchange('binance' as any, options as any, store())],
    ['binanceinverse', () => new BinanceInverseExchange('binanceinverse' as any, options as any, store())],
    ['binancepm', () => new BinancePMExchange('binancepm' as any, options as any, store())],
    ['binancespot', () => new BinanceSpotExchange('binancespot' as any, options as any, store())],
    ['bingx', () => new BingxExchange('bingx' as any, options as any, store())],
    ['bitget', () => new BitgetExchange('bitget' as any, options as any, store())],
    ['bitrue', () => new BitrueExchange('bitrue' as any, options as any, store())],
    ['bitunix', () => new BitunixExchange('bitunix' as any, options as any, store())],
    ['blofin', () => new BlofinExchange('blofin' as any, options as any, store())],
    ['bybit', () => new BybitExchange('bybit' as any, options as any, store())],
    ['bybitinverse', () => new BybitInverseExchange('bybitinverse' as any, options as any, store())],
    ['bybitspot', () => new BybitSpotExchange('bybitspot' as any, options as any, store())],
    ['coinbase', () => new CoinbaseExchange('coinbase' as any, options as any, store())],
    ['cryptocom', () => new CryptocomExchange('cryptocom' as any, options as any, store())],
    ['grvt', () => new GrvtExchange('grvt' as any, options as any, store())],
    ['hyperliquid', () => new HyperliquidExchange('hyperliquid' as any, options as any, store())],
    ['hyperliquidspot', () => new HyperliquidSpotExchange('hyperliquidspot' as any, options as any, store())],
    ['kraken', () => new KrakenExchange('kraken' as any, options as any, store())],
    ['krakenspot', () => new KrakenSpotExchange('krakenspot' as any, options as any, store())],
    ['kucoin', () => new KucoinExchange('kucoin' as any, options as any, store())],
    ['lighter', () => new LighterExchange('lighter' as any, options as any, store())],
    ['mexc', () => new MexcExchange('mexc' as any, options as any, store())],
    ['okx', () => new OKXExchange('okx' as any, options as any, store())],
    ['orderly', () => new OrderlyExchange('orderly' as any, options as any, store())],
    ['paradex', () => new ParadexExchange('paradex' as any, options as any, store())],
    ['phemex', () => new PhemexExchange('phemex' as any, options as any, store())],
    ['weex', () => new WeexExchange('weex' as any, options as any, store())],
    ['woo', () => new WOOXExchange('woo' as any, options as any, store())],
  ];
  const plain = {
    symbol: 'BTCUSDT',
    type: OrderType.Limit,
    side: OrderSide.Buy,
    amount: 1,
    price: 90,
  } as PlaceOrderOpts;
  const shapes: Partial<PlaceOrderOpts>[] = [
    { type: OrderType.Limit, price: 110, reduceOnly: true, stopLoss: 120 },
    { type: OrderType.Market, reduceOnly: true, takeProfit: 80 },
    { type: OrderType.Limit, price: 110, close: true, stopLoss: 0, takeProfit: 80 },
  ];

  for (const [venue, make] of subjects) {
    it(venue, async () => {
      for (const shape of shapes) {
        const exchange = make();
        exchange.emitAndLogError = vi.fn();
        // Any venue request means the order got past the gate.
        const request = vi.fn(() => {
          throw new Error('reached the venue');
        });
        exchange.xhr = new Proxy({}, { get: () => request });
        const bracket = { ...plain, side: OrderSide.Sell, ...shape } as PlaceOrderOpts;
        await expect(exchange.placeOrder(bracket)).rejects.toThrow(REDUCE_ONLY_LEGS_REFUSAL);
        await expect(exchange.placeOrders([plain, bracket])).rejects.toThrow(REDUCE_ONLY_LEGS_REFUSAL);
        expect(request).not.toHaveBeenCalled();
      }
    });
  }
});
