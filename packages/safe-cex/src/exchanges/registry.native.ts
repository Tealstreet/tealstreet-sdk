import type { ExchangeName } from '../types';
import type { BaseExchange } from './base';

declare const require: <T = any>(id: string) => T;

type ExchangeLoader = () => Promise<typeof BaseExchange>;

const load =
  (loader: () => typeof BaseExchange): ExchangeLoader =>
  () =>
    Promise.resolve(loader());

export const exchangeRegistry: Record<ExchangeName, ExchangeLoader> = {
  apexomni: load(() => require('../__tests__/reference/apexomni/apexomni.exchange').ApexOmniExchange),
  asterdex: load(() => require('../__tests__/reference/asterdex/asterdex.exchange').AsterDEXExchange),
  backpack: load(() => require('../__tests__/reference/backpack/backpack.exchange').BackpackExchange),
  binance: load(() => require('../__tests__/reference/binance/binance.exchange').BinanceExchange),
  binanceinverse: load(() => require('../__tests__/reference/binance/binanceinverse.exchange').BinanceInverseExchange),
  binanceoptions: load(() => require('../__tests__/reference/binance/binanceoptions.exchange').BinanceOptionsExchange),
  binancepm: load(() => require('../__tests__/reference/binance/binancepm.exchange').BinancePMExchange),
  binancespot: load(() => require('../__tests__/reference/binance/binancespot.exchange').BinanceSpotExchange),
  bingx: load(() => require('../__tests__/reference/bingx/bingx.exchange').BingxExchange),
  bitget: load(() => require('../__tests__/reference/bitget/bitget.exchange').BitgetExchange),
  bitgetinverse: load(() => require('../__tests__/reference/bitget/bitgetinverse.exchange').BitgetInverseExchange),
  bitgetusdc: load(() => require('../__tests__/reference/bitget/bitgetusdc.exchange').BitgetUSDCExchange),
  bitrue: load(() => require('../__tests__/reference/bitrue/bitrue.exchange').BitrueExchange),
  bitunix: load(() => require('../__tests__/reference/bitunix/bitunix.exchange').BitunixExchange),
  blofin: load(() => require('../__tests__/reference/blofin/blofin.exchange').BlofinExchange),
  breakoutprop: load(() => require('../__tests__/reference/breakoutprop/breakoutprop.exchange').BreakoutpropExchange),
  bybit: load(() => require('../__tests__/reference/bybit/bybit.exchange').BybitExchange),
  bybitdemo: load(() => require('../__tests__/reference/bybit/bybitdemo.exchange').BybitDemoExchange),
  hyrotrader: load(() => require('../__tests__/reference/bybit/hyrotrader.exchange').HyrotraderExchange),
  bybitinverse: load(() => require('../__tests__/reference/bybit/bybitinverse.exchange').BybitInverseExchange),
  bybitspot: load(() => require('../__tests__/reference/bybit/bybitspot.exchange').BybitSpotExchange),
  coinbase: load(() => require('./rust/rust.exchange').RustEngineExchange),
  coinbasespot: load(() => require('./rust/rust.exchange').RustEngineExchange),
  cryptocom: load(() => require('../__tests__/reference/cryptocom/cryptocom.exchange').CryptocomExchange),
  grvt: load(() => require('../__tests__/reference/grvt/grvt.exchange').GrvtExchange),
  hyperliquid: load(() => require('../__tests__/reference/hyperliquid/hyperliquid.exchange').HyperliquidExchange),
  hyperliquidspot: load(() => require('../__tests__/reference/hyperliquid/hyperliquidspot.exchange').HyperliquidSpotExchange),
  kraken: load(() => require('../__tests__/reference/kraken/kraken.exchange').KrakenExchange),
  krakenspot: load(() => require('../__tests__/reference/kraken/krakenspot.exchange').KrakenSpotExchange),
  kucoin: load(() => require('../__tests__/reference/kucoin/kucoin.exchange').KucoinExchange),
  kucoinspot: load(() => require('../__tests__/reference/kucoin/kucoinspot.exchange').KucoinSpotExchange),
  lighter: load(() => require('../__tests__/reference/lighter/lighter.exchange').LighterExchange),
  'lighter-robinhood': load(() => require('../__tests__/reference/lighter/lighter.exchange').LighterExchange),
  mexc: load(() => require('../__tests__/reference/mexc/mexc.exchange').MexcExchange),
  okx: load(() => require('../__tests__/reference/okx/okx.exchange').OKXExchange),
  okxdemo: load(() => require('../__tests__/reference/okx/okxdemo.exchange').OKXDemoExchange),
  okxeu: load(() => require('../__tests__/reference/okx/okxeu.exchange').OKXEUExchange),
  okxeudemo: load(() => require('../__tests__/reference/okx/okxeudemo.exchange').OKXEUDemoExchange),
  paradex: load(() => require('../__tests__/reference/paradex/paradex.exchange').ParadexExchange),
  phemex: load(() => require('../__tests__/reference/phemex/phemex.exchange').PhemexExchange),
  polymarket: load(() => require('../__tests__/reference/polymarket/polymarket.exchange').PolymarketExchange),
  weex: load(() => require('../__tests__/reference/weex/weex.exchange').WeexExchange),
  woo: load(() => require('../__tests__/reference/woo/woo.exchange').WOOXExchange),
  kodiakfi: load(() => require('./rust/rust.exchange').RustEngineExchange),
  mode: load(() => require('./rust/rust.exchange').RustEngineExchange),
  salsadex: load(() => require('./rust/rust.exchange').RustEngineExchange),
  'tealstreet-dex': load(
    () => require('./rust/rust.exchange').RustEngineExchange,
  ),
  txflow: load(() => require('./rust/rust.exchange').RustEngineExchange),
  what: load(() => require('./rust/rust.exchange').RustEngineExchange),
  woofi: load(() => require('./rust/rust.exchange').RustEngineExchange),
};

export const getRegisteredExchangeNames = (): ExchangeName[] => {
  return Object.keys(exchangeRegistry) as ExchangeName[];
};
