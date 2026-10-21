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
    leverageQuery: is('bingx')
      ? 'onReadAndMetadata'
      : is('asterdex', 'blofin', 'bitrue', 'grvt', 'paradex', 'txflow', 'coinbase', 'coinbasespot', 'polymarket', 'weex', ...OKX, ...ORDERLY)
        ? 'onRead'
        : is('mexc', 'woo', 'kraken')
          ? 'onMetadata'
          : 'settings',
    maxLeverage: is('bitrue') ? 'query' : is('polymarket') ? 'rails' : is('breakoutprop') ? 'unsupported' : 'market',
    accountLeverage: is(...ORDERLY),
    accountMarginType: is('blofin', 'kucoinspot'),
    cancelWithSymbols: is('asterdex', 'kucoinspot'),
    amendWithSymbol: is('kucoinspot'),
    orderBookQuery: is('asterdex', 'polymarket'),
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
    closedPositions: is('bybit', 'bybitdemo', 'hyrotrader', 'bybitinverse', 'blofin', ...OKX),
    balanceInfo: is('bybit', 'bybitdemo', 'hyrotrader', 'bybitinverse', 'bybitspot'),
    hostClock: is('backpack', 'coinbase', 'coinbasespot', ...ORDERLY),
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
