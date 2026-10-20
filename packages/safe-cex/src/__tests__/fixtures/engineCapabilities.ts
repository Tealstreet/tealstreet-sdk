import type { EngineCapabilities } from '@tealstreet/safe-cex-wasm/host';
import type { RustEngineExchange } from '../../exchanges/rust/rust.exchange';

const ORDERLY = ['tealstreet-dex', 'kodiakfi', 'mode', 'salsadex', 'what', 'woofi'];
const OKX = ['okx', 'okxeu', 'okxdemo', 'okxeudemo'];

/**
 * The record each real engine reports, for fake engines in bridge tests.
 * `sdk-rs/hosts/wasm` pins the real records (`capabilities_match_the_former_bridge_name_lists`).
 */
export const engineCapabilities = (venue: string, overrides: Partial<EngineCapabilities> = {}): string => {
  const is = (...names: string[]) => names.includes(venue);
  const noHistory = is('breakoutprop', 'hyperliquidspot', 'krakenspot');
  const noOrdersOrIncome = noHistory || is('apexomni', 'bitrue', 'kucoinspot');
  const record: EngineCapabilities = {
    account: is('apexomni', 'kucoinspot', 'asterdex', 'polymarket')
      ? 'query'
      : is('coinbase', 'coinbasespot', ...ORDERLY)
        ? 'afterLoad'
        : 'unsupported',
    clientOrderLookup: is('hyperliquid', 'grvt', 'paradex', 'bitrue', 'kucoinspot', 'phemex', 'binance', 'binancepm', ...OKX),
    fillHistory: !noHistory,
    orderHistory: !noOrdersOrIncome,
    incomeHistory: !noOrdersOrIncome,
    fillsByOrderId: is('grvt'),
    cancelAll: is('polymarket', 'breakoutprop', 'woo', 'grvt', 'kucoin', 'kucoinspot'),
    leverageQuery: is('asterdex', 'blofin', 'bitrue', 'grvt', 'paradex', 'txflow', 'coinbase', 'coinbasespot', 'polymarket', ...OKX, ...ORDERLY)
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
    ...overrides,
  };
  return JSON.stringify(record);
};

/** Adopts the exchange's record as engine creation would, for tests that inject their own host. */
export const adoptEngineCapabilities = (exchange: RustEngineExchange, overrides: Partial<EngineCapabilities> = {}) =>
  (exchange as unknown as { adoptCapabilities(record: EngineCapabilities): void }).adoptCapabilities(
    JSON.parse(engineCapabilities(exchange.name, overrides)),
  );
