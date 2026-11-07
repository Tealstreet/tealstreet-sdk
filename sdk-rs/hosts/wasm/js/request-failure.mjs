// Only literal API vocabulary survives; opaque path segments, queries and origins never do.
const SEGMENTS = new Set(('api v1 v2 v3 v4 v5 fapi dapi sapi openApi swap spot market public private account trade orders order positions position balance balances wallet instruments-info tickers realtime list info all wallet-balance fee-rate config funds assets time order-history cancel-all account-info batch instruments pending-algo asset orders-pending orders-tpsl-pending orders-algo-pending margin-mode position-mode batch-leverage-info').split(' '));
const METHODS = new Set(['GET', 'POST', 'PUT', 'PATCH', 'DELETE', 'HEAD', 'OPTIONS']);

export function requestFailureFields(packet) {
  let path = '/:unknown';
  try {
    const parts = new URL(packet.url).pathname.split('/').filter(Boolean);
    path = '/' + parts.slice(0, 8).map((part) => SEGMENTS.has(part) ? part : ':segment').join('/');
    if (parts.length > 8) path += '/:more';
    if (path.length > 160) path = '/:more';
  } catch { /* Invalid URLs disclose no caller text. */ }
  return { method: METHODS.has(packet.method) ? packet.method : 'OTHER', path };
}
