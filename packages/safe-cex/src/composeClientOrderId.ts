import type { ExchangeName } from './types';

import { BROKER_ID as BINANCE_BROKER_ID } from './exchanges/binance/binance.types';
import { BROKER_ID as BLOFIN_BROKER_ID } from './exchanges/blofin/blofin.types';
import { BROKER_ID as OKX_BROKER_ID } from './exchanges/okx/okx.types';

/**
 * Broker prefixes prepended to caller-supplied client order IDs. Venues absent
 * from this map send the caller ID unmodified (their attribution is not
 * ID-embedded). The composed value is the wire ID: exact-ID lookups must use
 * composeClientOrderId(exchangeName, callerId), never the bare caller ID.
 */
export const CLIENT_ORDER_ID_BROKER_PREFIXES: Partial<Record<ExchangeName, string>> = {
  binance: BINANCE_BROKER_ID,
  okx: OKX_BROKER_ID,
  okxeu: OKX_BROKER_ID,
  blofin: BLOFIN_BROKER_ID,
};

export const composeClientOrderId = (exchangeName: ExchangeName, callerId: string): string =>
  `${CLIENT_ORDER_ID_BROKER_PREFIXES[exchangeName] ?? ''}${callerId}`;
