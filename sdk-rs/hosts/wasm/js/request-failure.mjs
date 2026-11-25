// Only literal API vocabulary survives; opaque path segments, queries and origins never do.
const SEGMENTS = new Set(('api v1 v2 v3 v4 v5 fapi dapi sapi openApi swap spot market public private account trade orders order positions position balance balances wallet instruments-info tickers realtime list info all wallet-balance fee-rate config funds assets time order-history cancel-all account-info batch instruments pending-algo asset orders-pending orders-tpsl-pending orders-algo-pending margin-mode position-mode batch-leverage-info').split(' '));
const METHODS = new Set(['GET', 'POST', 'PUT', 'PATCH', 'DELETE', 'HEAD', 'OPTIONS']);

/** Closed labels only: never stringify a provider error, its config or nested cause. */
export function transportFailureMessage(error) {
  let cause = 'unknown';
  try {
    // Never invoke error accessors. Descriptor traps can reenter, so callers
    // recheck resource liveness before publishing the resulting closed label.
    const own = (key) => Object.getOwnPropertyDescriptor(error, key)?.value;
    let name = own('name');
    if (name === undefined && error instanceof TypeError) name = 'TypeError';
    if (name === undefined && typeof DOMException === 'function' && error instanceof DOMException)
      name = Object.getOwnPropertyDescriptor(DOMException.prototype, 'name').get.call(error);
    switch (name) {
      case 'TradeTunnelTimeoutError': case 'TradeTunnelReadinessError': case 'TimeoutError': cause = 'timeout'; break;
      case 'TradeTunnelCanceledError': case 'AbortError': cause = 'aborted'; break;
      case 'TradeTunnelDisposedError': cause = 'disposed'; break;
      case 'TradeTunnelResponseDecodeError': cause = 'response-decode'; break;
      case 'TradeTunnelError':
        cause = own('originalError') === 'TradeTunnel connection lost, reconnecting...' ? 'disconnect' : 'tunnel';
        break;
      case 'TypeError': {
        const message = own('message');
        if (message === 'Failed to fetch' || message === 'fetch failed' || message === 'Load failed'
          || message === 'NetworkError when attempting to fetch resource.') cause = 'direct-fetch';
      }
    }
  } catch { /* Hostile descriptors and foreign errors disclose no caller text. */ }
  return `HTTP transport request failed (cause: ${cause})`;
}


// Encoded class names survive the canonical SyntaxError text scrub without an exemption.
const ERROR_NAMES = new Map([
  ['Error', 'error'], ['TypeError', 'type_error'], ['SyntaxError', 'syntax_error'], ['RangeError', 'range_error'],
  ['URIError', 'uri_error'], ['ReferenceError', 'reference_error'], ['EvalError', 'eval_error'],
  ['AbortError', 'abort_error'], ['TimeoutError', 'timeout_error'], ['ProxyTokenUnavailableError', 'proxy_token_unavailable'],
  ['TradeTunnelTimeoutError', 'tunnel_timeout'], ['TradeTunnelReadinessError', 'tunnel_readiness'],
  ['TradeTunnelCanceledError', 'tunnel_canceled'], ['TradeTunnelDisposedError', 'tunnel_disposed'],
  ['TradeTunnelResponseDecodeError', 'tunnel_response_decode'], ['TradeTunnelError', 'tunnel_error'],
]);
/** A finite class label; never call provider accessors or stringify a cause. */
export function failureErrorName(error) {
  try {
    const name = Object.getOwnPropertyDescriptor(error, 'name');
    if (name) return ERROR_NAMES.get(name.value) ?? 'other';
    if (typeof DOMException === 'function' && error instanceof DOMException) {
      const value = Object.getOwnPropertyDescriptor(DOMException.prototype, 'name').get.call(error);
      return ERROR_NAMES.get(value) ?? 'other';
    }
    for (const Class of [SyntaxError, TypeError, RangeError, URIError, ReferenceError, EvalError, Error])
      if (error instanceof Class) return ERROR_NAMES.get(Class.name);
  } catch { /* Foreign objects disclose no caller text. */ }
  return 'other';
}

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
