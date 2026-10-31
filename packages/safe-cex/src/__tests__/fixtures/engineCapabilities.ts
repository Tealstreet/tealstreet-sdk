import type { EngineCapabilities } from '@tealstreet/safe-cex-wasm/host';

import { QUANTITY_UNITS } from '@tealstreet/safe-cex-wasm/wire';
import type { RustEngineExchange } from '../../exchanges/rust/rust.exchange';

const ORDERLY = ['tealstreet-dex', 'kodiakfi', 'mode', 'salsadex', 'what', 'woofi'];
const OKX = ['okx', 'okxeu', 'okxdemo', 'okxeudemo'];

/**
 * The record each real engine reports, for fake engines in bridge tests.
 * `sdk-rs/hosts/wasm` pins the real records (`capabilities_match_the_former_bridge_name_lists`).
 */
export const engineCapabilities = (venue: string, overrides: Partial<EngineCapabilities> = {}): string => {
  const is = (...names: string[]) => names.includes(venue);
  const noHistory = is('breakoutprop', 'hyperliquidspot', 'krakenspot', 'binancespot', 'binanceinverse');
  const noOrdersOrIncome = noHistory || is('apexomni', 'bitrue', 'kucoinspot');
  const record: EngineCapabilities = {
    account: is(
      'apexomni', 'asterdex', 'backpack', 'binance', 'binancepm', 'bingx', 'bitget', 'bitgetinverse', 'bitgetusdc',
      'bybit', 'bybitinverse', 'bybitspot', 'bybitdemo', 'cryptocom', 'grvt', 'kucoin', 'kucoinspot', 'lighter',
      'lighter-robinhood', 'mexc', 'paradex', 'phemex', 'polymarket', 'weex', 'woo',
    )
      ? 'query'
      : is('coinbase', 'coinbasespot', ...ORDERLY)
        ? 'afterLoad'
        : 'unsupported',
    clientOrderLookup: is('hyperliquid', 'grvt', 'paradex', 'bitrue', 'kucoinspot', 'phemex', 'binance', 'binancepm', ...OKX),
    fillHistory: !noHistory,
    orderHistory: !noOrdersOrIncome,
    incomeHistory: !noOrdersOrIncome,
    fillsByOrderId: is('grvt'),
    cancelAll: is('polymarket', 'breakoutprop', 'woo', 'grvt', 'kucoin', 'kucoinspot', ...ORDERLY)
      ? 'native'
      : is('phemex', 'asterdex', 'bitget', 'bitgetinverse', 'bitgetusdc', 'blofin', 'bitrue', ...OKX)
        ? 'eachPlacedSymbol'
        : 'eachOpenSymbol',
    leverageQuery: is('bingx', 'weex', 'bybit', 'bybitinverse', 'bybitdemo', 'bitget', 'bitgetinverse', 'bitgetusdc', 'kucoin')
      ? 'onReadAndMetadata'
      : is('asterdex', 'blofin', 'bitrue', 'grvt', 'paradex', 'txflow', 'coinbase', 'coinbasespot', 'polymarket', ...OKX, ...ORDERLY)
        ? 'onRead'
        : is('mexc', 'woo', 'kraken')
          ? 'onMetadata'
          : 'settings',
    maxLeverage: is('bitrue') ? 'query' : is('polymarket') ? 'rails' : is('breakoutprop') ? 'unsupported' : 'market',
    accountLeverage: is(...ORDERLY),
    accountMarginType: is('blofin', 'kucoinspot'),
    cancelWithSymbols: is('asterdex', 'kucoinspot'),
    amendWithSymbol: is('kucoinspot'),
    orderBookQuery: is('asterdex', 'polymarket', 'mexc', 'backpack', 'cryptocom', 'grvt', 'paradex', 'weex'),
    options: false,
    quantityUnits: is('mexc', 'blofin', 'kucoin', 'coinbase', 'coinbasespot')
      ? 'nativePlacement'
      : is('bitgetinverse')
        ? 'inverseNative'
        : is(...OKX)
          ? 'contractCount'
          : is('binanceinverse')
            ? 'inverseContractSized'
            : 'base',
    accountTier: is('lighter', 'lighter-robinhood'),
    closedPositions: is('bybit', 'bybitdemo', 'hyrotrader', 'bybitinverse', 'blofin', 'bitunix', ...OKX),
    balanceInfo: is('bybit', 'bybitdemo', 'hyrotrader', 'bybitinverse', 'bybitspot', 'binanceinverse'),
    marketRefresh: is('bybit', 'bybitdemo', 'bybitinverse', 'bybitspot', 'hyrotrader'),
    positionHistory: is('kraken', 'bitget', 'bitgetusdc', 'bitgetinverse') ? 'fillsAccount'
      : is('binance', 'hyperliquid', 'paradex') ? 'fillsSymbol'
      : is('bybit', 'bybitdemo', 'hyrotrader') ? 'bybit'
      : is(...OKX) ? 'okx' : is('blofin') ? 'blofin' : is('woo') ? 'wooOrders' : is('bitunix') ? 'bitunix' : 'unsupported',
    closedPositionFormat: is('bybit', 'bybitdemo', 'hyrotrader', 'bybitinverse') ? 'bybit'
      : is(...OKX) ? 'okx' : is('blofin') ? 'blofin' : is('bitunix') ? 'bitunix' : 'unsupported',
    fillHistoryTime: is('woo') ? 'nonnegativeSecondsOrMilliseconds'
      : is('apexomni', 'blofin', 'bitrue', 'grvt', 'hyperliquid', 'paradex', 'lighter', 'lighter-robinhood', 'kraken', 'coinbase', 'coinbasespot')
        ? 'secondsOrMilliseconds' : 'asProvided',
    stpControls: is('asterdex'),
    bybitExecutionControls: is('bybit', 'bybitinverse', 'bybitspot', 'bybitdemo', 'hyrotrader'),
    rejectClientSlippage: is('hyrotrader'),
    finiteCommandNumbers: is('coinbase', 'coinbasespot'),
    finiteHistoryBounds: is('coinbase', 'coinbasespot'),
    rejectOrderHistoryFilters: is('txflow', 'coinbase', 'coinbasespot', ...ORDERLY),
    publicCatalogueValidation: is('breakoutprop'),
    isolatedBookSubscriptions: is('asterdex'),
    scheduleCancel: is('hyperliquid'),
    inverseClosedPositions: is('bybitinverse'),
    ...overrides,
  };
  return JSON.stringify(record);
};

/** Adopts the exchange's record as engine creation would, for tests that inject their own host. */
export const adoptEngineCapabilities = (exchange: RustEngineExchange, overrides: Partial<EngineCapabilities> = {}) =>
  (exchange as unknown as { adoptEngineCapabilities(record: EngineCapabilities): void }).adoptEngineCapabilities(
    JSON.parse(engineCapabilities(exchange.name, overrides)),
  );

/** The venue's market quantity-units code, as the engine writes it into MARKETS flags. */
export const quantityUnitsCode = (venue: string): number =>
  QUANTITY_UNITS.indexOf((JSON.parse(engineCapabilities(venue)) as EngineCapabilities).quantityUnits);
