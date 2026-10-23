import type { ExchangeName } from '../types';
import type { BaseExchange } from './base';

declare const require: <T = any>(id: string) => T;

type ExchangeLoader = () => Promise<typeof BaseExchange>;

const load =
  (loader: () => typeof BaseExchange): ExchangeLoader =>
  () =>
    Promise.resolve(loader());

export const exchangeRegistry: Record<ExchangeName, ExchangeLoader> = {
  apexomni: load(() => require('./apexomni/apexomni.exchange').ApexOmniExchange),
  asterdex: load(() => require('./asterdex/asterdex.exchange').AsterDEXExchange),
  backpack: load(() => require('./backpack/backpack.exchange').BackpackExchange),
  binance: load(() => require('./binance/binance.exchange').BinanceExchange),
  binanceinverse: load(() => require('./binance/binanceinverse.exchange').BinanceInverseExchange),
  binanceoptions: load(() => require('./binance/binanceoptions.exchange').BinanceOptionsExchange),
  binancepm: load(() => require('./binance/binancepm.exchange').BinancePMExchange),
  binancespot: load(() => require('./binance/binancespot.exchange').BinanceSpotExchange),
  bingx: load(() => require('./bingx/bingx.exchange').BingxExchange),
  bitget: load(() => require('./bitget/bitget.exchange').BitgetExchange),
  bitgetinverse: load(() => require('./bitget/bitgetinverse.exchange').BitgetInverseExchange),
  bitgetusdc: load(() => require('./bitget/bitgetusdc.exchange').BitgetUSDCExchange),
  bitrue: load(() => require('./bitrue/bitrue.exchange').BitrueExchange),
  bitunix: load(() => require('./bitunix/bitunix.exchange').BitunixExchange),
  blofin: load(() => require('./blofin/blofin.exchange').BlofinExchange),
  breakoutprop: load(() => require('./breakoutprop/breakoutprop.exchange').BreakoutpropExchange),
  bybit: load(() => require('./bybit/bybit.exchange').BybitExchange),
  bybitdemo: load(() => require('./bybit/bybitdemo.exchange').BybitDemoExchange),
  hyrotrader: load(() => require('./bybit/hyrotrader.exchange').HyrotraderExchange),
  bybitinverse: load(() => require('./bybit/bybitinverse.exchange').BybitInverseExchange),
  bybitspot: load(() => require('./bybit/bybitspot.exchange').BybitSpotExchange),
  coinbase: load(() => require('./rust/rust.exchange').RustEngineExchange),
  coinbasespot: load(() => require('./rust/rust.exchange').RustEngineExchange),
  cryptocom: load(() => require('./cryptocom/cryptocom.exchange').CryptocomExchange),
  grvt: load(() => require('./grvt/grvt.exchange').GrvtExchange),
  hyperliquid: load(() => require('./hyperliquid/hyperliquid.exchange').HyperliquidExchange),
  hyperliquidspot: load(() => require('./hyperliquid/hyperliquidspot.exchange').HyperliquidSpotExchange),
  kraken: load(() => require('./kraken/kraken.exchange').KrakenExchange),
  krakenspot: load(() => require('./kraken/krakenspot.exchange').KrakenSpotExchange),
  kucoin: load(() => require('./kucoin/kucoin.exchange').KucoinExchange),
  kucoinspot: load(() => require('./kucoin/kucoinspot.exchange').KucoinSpotExchange),
  lighter: load(() => require('./lighter/lighter.exchange').LighterExchange),
  'lighter-robinhood': load(() => require('./lighter/lighter.exchange').LighterExchange),
  mexc: load(() => require('./mexc/mexc.exchange').MexcExchange),
  okx: load(() => require('./okx/okx.exchange').OKXExchange),
  okxdemo: load(() => require('./okx/okxdemo.exchange').OKXDemoExchange),
  okxeu: load(() => require('./okx/okxeu.exchange').OKXEUExchange),
  okxeudemo: load(() => require('./okx/okxeudemo.exchange').OKXEUDemoExchange),
  paradex: load(() => require('./paradex/paradex.exchange').ParadexExchange),
  phemex: load(() => require('./phemex/phemex.exchange').PhemexExchange),
  polymarket: load(() => require('./polymarket/polymarket.exchange').PolymarketExchange),
  weex: load(() => require('./weex/weex.exchange').WeexExchange),
  woo: load(() => require('./woo/woo.exchange').WOOXExchange),
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
