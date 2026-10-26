import type { IsReadRequest } from '../../../utils/retryReadsOnly';

import { ENDPOINTS } from '../../../exchanges/grvt/grvt.types';

// GRVT POSTs its reads too, so reads are named. Matched on the last path segment
// because the TDG client posts some under `/lite/v1/` and others under `/full/v1/`.
const lastSegment = (path: string) => path.split('?')[0].split('/').pop() ?? '';

const GRVT_READS = new Set(
  [
    ENDPOINTS.INSTRUMENTS,
    ENDPOINTS.INSTRUMENT,
    ENDPOINTS.TICKER,
    ENDPOINTS.MINI_TICKER,
    ENDPOINTS.ORDERBOOK,
    ENDPOINTS.TRADES,
    ENDPOINTS.KLINES,
    ENDPOINTS.FUNDING,
    ENDPOINTS.AUTH,
    ENDPOINTS.ACCOUNT_SUMMARY,
    ENDPOINTS.POSITIONS,
    ENDPOINTS.OPEN_ORDERS,
    ENDPOINTS.ORDER_HISTORY,
    ENDPOINTS.FILL_HISTORY,
    ENDPOINTS.GET_LEVERAGE,
    ENDPOINTS.GET_AUTHORIZED_BUILDERS,
    ENDPOINTS.BUILDER_FILL_HISTORY,
    '/full/v1/aggregated_account_summary',
    '/full/v1/funding_account_summary',
  ].map(lastSegment),
);

export const isGrvtRead: IsReadRequest = (config) => GRVT_READS.has(lastSegment(config.url ?? ''));
