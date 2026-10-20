/**
 * api.constants.* — frozen literal-string enums every script can use.
 * See SPEC §2.1.
 *
 * - String enums (OrderSide, OrderType, …) come straight from safe-cex; they
 *   compile to plain `{ Buy: 'buy', Sell: 'sell' }` objects, structured-clone
 *   safe, and act as single source of truth for both web and worker boundaries.
 * - ExchangeName is a literal-string TS union (not an enum). The runtime
 *   `EXCHANGE_NAME_MAP` below is built explicitly and frozen — must stay in
 *   step with `@tealstreet/safe-cex/src/types` `ExchangeName`. A
 *   `_check` line catches drift at typecheck time.
 */
import {
  MarginType,
  OrderSide,
  OrderStatus,
  OrderStopTriggerType,
  OrderTimeInForce,
  OrderType,
  PositionMode,
  PositionSide,
  type ExchangeName,
} from '@tealstreet/safe-cex/src/types';

import type { CoreConstants, ExchangeNameMap } from './api/CoreApi.types';

const EXCHANGE_NAMES = [
  'apexomni',
  'asterdex',
  'backpack',
  'binance',
  'binanceinverse',
  'binanceoptions',
  'binancespot',
  'binancepm',
  'bingx',
  'bitget',
  'bitgetinverse',
  'bitgetusdc',
  'bitrue',
  'bitunix',
  'blofin',
  'breakoutprop',
  'bybit',
  'bybitdemo',
  'bybitinverse',
  'bybitspot',
  'hyrotrader',
  'coinbase',
  'coinbasespot',
  'cryptocom',
  'grvt',
  'hyperliquid',
  'hyperliquidspot',
  'lighter',
  'lighter-robinhood',
  'mexc',
  'okx',
  'okxdemo',
  'okxeu',
  'okxeudemo',
  'paradex',
  'phemex',
  'polymarket',
  'kodiakfi',
  'mode',
  'salsadex',
  'tealstreet-dex',
  'txflow',
  'what',
  'weex',
  'woo',
  'woofi',
  'kucoin',
  'kucoinspot',
  'kraken',
  'krakenspot',
] as const satisfies readonly ExchangeName[];

// Typecheck-time drift detector: every ExchangeName union member must appear
// in EXCHANGE_NAMES, and EXCHANGE_NAMES must contain only ExchangeName values.
// If safe-cex adds/removes an exchange, this line forces a script-api-core
// follow-up commit.
type _ExchangeNameDrift =
  Exclude<ExchangeName, (typeof EXCHANGE_NAMES)[number]> | Exclude<(typeof EXCHANGE_NAMES)[number], ExchangeName>;
const _check: _ExchangeNameDrift extends never ? true : false = true;
void _check;

const buildExchangeNameMap = (): ExchangeNameMap => {
  const out: Record<string, ExchangeName> = {};
  for (const name of EXCHANGE_NAMES) {
    out[name] = name;
  }
  return Object.freeze(out) as ExchangeNameMap;
};

const ExchangeName: ExchangeNameMap = buildExchangeNameMap();

export const CORE_CONSTANTS: CoreConstants = Object.freeze({
  ExchangeName,
  OrderSide,
  OrderType,
  OrderStatus,
  OrderTimeInForce,
  OrderStopTriggerType,
  PositionSide,
  PositionMode,
  MarginType,
});
