// Stable request identity. Caller IDs remain exact; generated IDs and transport
// signatures/clocks are nondeterministic. Economic action fields stay intact.
const CLOCKS = new Set(['timestamp', 'recvWindow', 'signature', 'requestId']);
const URL_VOLATILE = new Set([...CLOCKS, 'requestId']);
const CLIENT_IDS = new Set(['newClientOrderId', 'clientAlgoId', 'clientOrderId']);

// Timed replies describe recorded state; polling must not consume its transitions.
// Unstamped replies in a timed queue represent the state available at startup.
export const isWriteRequest = (method, url) => {
  method = method.toUpperCase();
  if (['GET', 'HEAD', 'OPTIONS'].includes(method)) return false;
  const parsed = new URL(url);
  if (['POST', 'PUT', 'DELETE'].includes(method) && /\/(listenKey|userDataStream)$/.test(parsed.pathname)) return false;
  // Hyperliquid's /info POSTs are reads, including account/history polling.
  return !(
    method === 'POST' &&
    parsed.pathname === '/info' &&
    /(^|\.)hyperliquid(?:-testnet)?\.xyz$/.test(parsed.hostname)
  );
};

export const selectResponse = (queue, elapsed, { write = false, commandIndex = 0 } = {}) => {
  if (!queue?.length) return undefined;
  const at = (record) => record.availableAt ?? record.effectiveAt ?? record.availableAfter ?? 0;
  const available = (record) => (record.afterCommand ?? 0) <= commandIndex && at(record) <= elapsed;
  if (write) {
    // An acknowledgement proves one attempt. Never replay it for another write,
    // or skip an earlier outstanding write to consume a later staged one.
    return available(queue[0]) ? queue.shift() : undefined;
  }
  const eligible = queue.filter(available);
  if (!eligible.length) return undefined;
  const stage = Math.max(...eligible.map((record) => record.afterCommand ?? 0));
  const current = eligible.filter((record) => (record.afterCommand ?? 0) === stage);
  if (
    !current.some(
      (record) =>
        record.availableAt !== undefined || record.effectiveAt !== undefined || record.availableAfter !== undefined,
    )
  ) {
    const record = current[0];
    if (current.length > 1) queue.splice(queue.indexOf(record), 1);
    return record;
  }
  let selected;
  let latest = -Infinity;
  for (const record of current) {
    const available = at(record);
    if (available <= elapsed && available >= latest) {
      selected = record;
      latest = available;
    }
  }
  return selected;
};

const parseJson = (text) =>
  JSON.parse(text, (_key, value, context) =>
    typeof value === 'number' && Number.isInteger(value) && !Number.isSafeInteger(value)
      ? JSON.rawJSON(context.source)
      : value,
  );

export const callerIdsOf = (records) => {
  const ids = new Set();
  const collect = (value) => {
    if (!value || typeof value !== 'object') return;
    if (typeof value.clientOrderId === 'string') ids.add(value.clientOrderId);
    for (const nested of Object.values(value)) collect(nested);
  };
  for (const record of records) if (record.type === 'command') collect(record);
  return ids;
};

// Body fields below the envelope remain economic data unless a venue opts
// into volatile body fields. Query JSON follows transport-query normalization.
const normalize = (value, callerIds, root = true, options = {}, path = []) => {
  if (typeof value === 'bigint') return JSON.rawJSON(String(value));
  if (JSON.isRawJSON(value)) return value;
  if (Array.isArray(value)) return value.map((item) => normalize(item, callerIds, false, options, [...path, '*']));
  if (value && typeof value === 'object') return Object.fromEntries(Object.keys(value).sort()
    .filter((key) => options.defaultBodyValues?.[key] !== value[key])
    .filter((key) => !options.ignoreGeneratedBodyId?.(key, value[key], callerIds, options.bodyContext))
    .filter((key) => {
      if (callerIds.has(value[key])) return true;
      if (options.generatedId?.(key, JSON.isRawJSON(value[key]) ? value[key].rawJSON : String(value[key]))) return false;
      if ((root || options.query || options.volatileBodyFields)
        && (CLOCKS.has(key) || (CLIENT_IDS.has(key) && !options.generatedId))) return false;
      if (root && options.hyperliquidExchange && key === 'nonce' && value.action) return false;
      // `c` is a minted cloid only on newly placed Hyperliquid order wires.
      // Cancel targets and unrelated fields named c remain exact.
      return !(options.hyperliquidExchange && key === 'c' && typeof value[key] === 'string' && /^0x[0-9a-f]{32}$/i.test(value[key])
        && ((options.hyperliquidAction === 'order' && path.join('.') === 'action.orders.*')
          || (options.hyperliquidAction === 'batchModify' && path.join('.') === 'action.modifies.*.order')));
    })
    .map((key) => [key, !callerIds.has(value[key]) && options.generatedBodyId?.(key, value[key])
      ? '<generated>' : normalize(value[key], callerIds, false, options, [...path, key])]));
  return value;
};

const canonicalParams = (params, callerIds, options = {}) => new URLSearchParams([...params]
  .filter(([key, value]) => callerIds.has(value) || (!URL_VOLATILE.has(key)
    && !options.generatedId?.(key, value) && (!CLIENT_IDS.has(key) || options.generatedId)))
  .filter(([, value]) => !(value === '' && options.cacheBust))
  .map(([key, value]) => {
    if (CLIENT_IDS.has(key)) return [key, value];
    if (!/^[\[{]/.test(value.trimStart())) return [key, value];
    try { return [key, JSON.stringify(normalize(parseJson(value), callerIds, false, { ...options, query: true }))]; }
    catch { return [key, value]; }
  })
  .sort(([a, av], [b, bv]) => a.localeCompare(b) || av.localeCompare(bv))).toString();

export const canonicalUrl = (url, callerIds = new Set(), options = {}) => {
  const parsed = new URL(url);
  return `${parsed.origin}${parsed.pathname}?${canonicalParams(parsed.searchParams, callerIds,
    { ...options, cacheBust: options.cacheBustPaths?.includes(parsed.pathname) })}`;
};

export const requestKey = (method, url, body, callerIds = new Set(), options = {}) => {
  method = method.toUpperCase();
  if (options.requestOptions) options = { ...options, ...options.requestOptions(method, new URL(url)) };
  const prefix = `${method.toUpperCase()} ${canonicalUrl(options.requestUrl?.(url, method) ?? url, callerIds, options)}`;
  if (body == null || body === '') return prefix;
  if (ArrayBuffer.isView(body) || body instanceof ArrayBuffer) body = new TextDecoder().decode(body);
  if (body instanceof URLSearchParams) {
    if (!options.requestBody) return `${prefix} ${canonicalParams(body, callerIds, options)}`;
    body = Object.fromEntries(body);
  }
  let parsed;
  try {
    parsed = typeof body === 'string' ? parseJson(body) : body;
  } catch {
    if (typeof body === 'string' && body.includes('=')) {
      const form = new URLSearchParams(body);
      if (options.requestBody) parsed = Object.fromEntries(form);
      else return `${prefix} ${canonicalParams(form, callerIds, options)}`;
    } else return `${prefix} ${body}`;
  }
  parsed = options.requestBody?.(parsed, { method, url, callerIds }) ?? parsed;
  const route = new URL(url);
  options = { ...options, hyperliquidAction: parsed?.action?.type, hyperliquidExchange: method.toUpperCase() === 'POST' && route.pathname === '/exchange'
    && /(^|\.)hyperliquid(?:-testnet)?\.xyz$/.test(route.hostname) };
  return `${prefix} ${JSON.stringify(normalize(parsed, callerIds, true, options.ignoreGeneratedBodyId
    ? { ...options, bodyContext: { method: method.toUpperCase(), url } } : options))}`;
};

export const subscriptionMessage = (message) => {
  const method = String(message.method ?? message.reqType ?? message.op ?? message.event ?? '').toUpperCase();
  if (!['SUBSCRIBE', 'UNSUBSCRIBE', 'SUB', 'UNSUB'].includes(method)) return undefined;
  const op = ['SUBSCRIBE', 'SUB'].includes(method) ? 'subscribe' : 'unsubscribe';
  const topics =
    message.params?.channels ??
    message.params ??
    message.args ??
    (typeof message.topic === 'string' ? [message.topic] : message.dataType ? [message.dataType] : []);
  return Array.isArray(topics) ? { op, topics } : undefined;
};

export const subscriptionKey = (url, text, callerIds = new Set(), options = {}) => {
  let message;
  if (options.bingxProtocol && ['Ping', 'Pong'].includes(text)) return undefined;
  try {
    message = typeof text === 'string' ? JSON.parse(text) : text;
  } catch {
    return `WS ${canonicalUrl(url, callerIds)} ${text}`;
  }
  message = options.subscriptionMessage?.(message) ?? message;
  const method = String(message?.method ?? message?.reqType ?? message?.op ?? message?.event ?? message?.type ?? '').toLowerCase();
  if (options.bitrueProtocol) {
    if (message?.ping !== undefined || message?.pong !== undefined) return undefined;
    if (message?.params?.cb_id !== undefined) {
      message = { ...message, params: { ...message.params } };
      delete message.params.cb_id;
    }
  }
  if (options.bingxProtocol && (message?.ping !== undefined || message?.pong !== undefined)) return undefined;
  if (options.cryptoProtocol && method === 'public/respond-heartbeat') return undefined;
  if (['ping', 'pong', 'heartbeat'].includes(method)) return undefined;
  if (message && typeof message === 'object') {
    message = { ...message };
    delete message.id;
    delete message.req_id;
    if (options.orderlyProtocol && method === 'auth' && message.params) {
      message.params = { ...message.params, timestamp: '<timestamp>', sign: '<signature>' };
    }
    // Crypto.com's audit validates auth and heartbeat identity before capture.
    if (options.cryptoProtocol) {
      delete message.nonce;
      if (method === 'public/auth') message.sig = '<signature>';
    }
    if (options.mexcProtocol && method === 'login' && message.param) {
      message.param = { ...message.param, reqTime: '<timestamp>', signature: '<signature>' };
    }
    // OKX login signatures and clocks are checked against credentials by Tape;
    // the outbound identity retains API key and passphrase for comparison.
    if (method === 'login' && Array.isArray(message.args)) {
      message.args = message.args.map((arg) => ({
        ...arg,
        timestamp: '<timestamp>',
        sign: '<signature>',
        ...(options.blofinProtocol ? { nonce: '<nonce>' } : {}),
      }));
    }
    if (options.wooProtocol && method === 'auth' && message.params) {
      message.params = { ...message.params, timestamp: '<timestamp>', sign: '<signature>' };
    }
    // Bybit signs a fresh expiration per connection. Keep the API identity.
    if (method === 'auth' && Array.isArray(message.args) && message.args.length === 3) {
      message.args = [message.args[0], '<expires>', '<signature>'];
    }
    if (['subscribe', 'unsubscribe'].includes(method) && Array.isArray(message.params)) {
      message.params = [...message.params].sort();
    }
    if (['subscribe', 'unsubscribe'].includes(method) && Array.isArray(message.args)) {
      message.args = [...message.args].sort();
    }
  }
  return `WS ${canonicalUrl(url, callerIds)} ${JSON.stringify(normalize(message, callerIds))}`;
};

// Subscription batches and one-topic sends have the same required feed set.
export const subscriptionKeys = (url, text, callerIds = new Set(), options = {}) => {
  let message;
  try {
    message = typeof text === 'string' ? parseJson(text) : text;
  } catch {
    const key = subscriptionKey(url, text, callerIds, options);
    return key === undefined ? [] : [key];
  }
  const method = String(message?.method ?? message?.reqType ?? message?.op ?? message?.event ?? message?.type ?? '').toLowerCase();
  if (['subscribe', 'unsubscribe'].includes(method)) {
    if (options.cryptoProtocol && Array.isArray(message.params?.channels)) {
      return message.params.channels.map((channel) =>
        subscriptionKey(url, { ...message, params: { ...message.params, channels: [channel] } }, callerIds, options),
      );
    }
    const field = Array.isArray(message.params) ? 'params' : Array.isArray(message.args) ? 'args' : undefined;
    if (field)
      return message[field].map((topic) => subscriptionKey(url, { ...message, [field]: [topic] }, callerIds, options));
  }
  const key = subscriptionKey(url, message, callerIds, options);
  return key === undefined ? [] : [key];
};

// Compatibility for existing helper callers; response selection stays centralized.
export const isRead = (method, url) => !isWriteRequest(method, url);
export const takeResponse = (queue, method, url, time = 0, commandIndex = Infinity, repeat = isRead(method, url)) =>
  selectResponse(queue, time, { commandIndex, write: !repeat });
