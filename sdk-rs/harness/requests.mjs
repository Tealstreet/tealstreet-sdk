// Stable request identity. Caller IDs remain exact; generated IDs and transport
// signatures/clocks are nondeterministic. Economic action fields stay intact.
const CLOCKS = new Set(['timestamp', 'recvWindow', 'signature', 'requestId']);
const URL_VOLATILE = new Set([...CLOCKS, 'requestId']);
const CLIENT_IDS = new Set(['newClientOrderId', 'clientAlgoId', 'clientOrderId', 'c']);

// Timed replies describe recorded state; polling must not consume its transitions.
// Unstamped replies in a timed queue represent the state available at startup.
export const isWriteRequest = (method, url) => {
  method = method.toUpperCase();
  if (['GET', 'HEAD', 'OPTIONS'].includes(method)) return false;
  const parsed = new URL(url);
  if (['POST', 'PUT', 'DELETE'].includes(method) && /\/(listenKey|userDataStream)$/.test(parsed.pathname)) return false;
  // Hyperliquid's /info POSTs are reads, including account/history polling.
  return !(method === 'POST' && parsed.pathname === '/info'
    && /(^|\.)hyperliquid(?:-testnet)?\.xyz$/.test(parsed.hostname));
};

export const selectResponse = (queue, elapsed, { write = false, commandIndex = 0 } = {}) => {
  if (!queue?.length) return undefined;
  const at = (record) => record.availableAt ?? record.effectiveAt ?? record.availableAfter ?? 0;
  const available = (record) => (record.afterCommand ?? 0) <= commandIndex
    && at(record) <= elapsed;
  if (write) {
    // An acknowledgement proves one attempt. Never replay it for another write,
    // or skip an earlier outstanding write to consume a later staged one.
    return available(queue[0]) ? queue.shift() : undefined;
  }
  const eligible = queue.filter(available);
  if (!eligible.length) return undefined;
  const stage = Math.max(...eligible.map((record) => record.afterCommand ?? 0));
  const current = eligible.filter((record) => (record.afterCommand ?? 0) === stage);
  if (!current.some((record) => record.availableAt !== undefined || record.effectiveAt !== undefined || record.availableAfter !== undefined)) {
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

const parseJson = (text) => JSON.parse(text, (_key, value, context) =>
  typeof value === 'number' && Number.isInteger(value) && !Number.isSafeInteger(value)
    ? JSON.rawJSON(context.source) : value);

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

const normalize = (value, callerIds, root = true, options = {}) => {
  if (JSON.isRawJSON(value)) return value;
  if (Array.isArray(value)) return value.map((item) => normalize(item, callerIds, false, options));
  if (value && typeof value === 'object') return Object.fromEntries(Object.keys(value).sort()
    .filter((key) => !(root && (CLOCKS.has(key) || (key === 'nonce' && value.action))))
    .filter((key) => options.defaultBodyValues?.[key] !== value[key])
    .filter((key) => !options.ignoreGeneratedBodyId?.(key, value[key], callerIds, options.bodyContext))
    .filter((key) => !CLIENT_IDS.has(key) || callerIds.has(value[key])
      || (options.generatedId && !options.generatedId(key, String(value[key]))))
    .map((key) => [key, !callerIds.has(value[key]) && options.generatedBodyId?.(key, value[key])
      ? '<generated>' : normalize(value[key], callerIds, false, options)]));
  return value;
};

const canonicalParams = (params, callerIds, options = {}) => new URLSearchParams([...params]
  .filter(([key]) => !URL_VOLATILE.has(key))
  .filter(([key, value]) => !CLIENT_IDS.has(key) || callerIds.has(value))
  .filter(([key, value]) => callerIds.has(value) || !options.generatedQueryId?.(key, value))
  .map(([key, value]) => {
    if (CLIENT_IDS.has(key)) return [key, value];
    if (!/^[\[{]/.test(value.trimStart())) return [key, value];
    try { return [key, JSON.stringify(normalize(parseJson(value), callerIds))]; }
    catch { return [key, value]; }
  })
  .sort(([a, av], [b, bv]) => a.localeCompare(b) || av.localeCompare(bv))).toString();

export const canonicalUrl = (url, callerIds = new Set(), options = {}) => {
  const parsed = new URL(url);
  return `${parsed.origin}${parsed.pathname}?${canonicalParams(parsed.searchParams, callerIds, options)}`;
};

export const requestKey = (method, url, body, callerIds = new Set(), options = {}) => {
  const prefix = `${method.toUpperCase()} ${canonicalUrl(options.requestUrl?.(url, method) ?? url, callerIds, options)}`;
  if (body == null || body === '') return prefix;
  if (ArrayBuffer.isView(body) || body instanceof ArrayBuffer) body = new TextDecoder().decode(body);
  if (body instanceof URLSearchParams) return `${prefix} ${canonicalParams(body, callerIds, options)}`;
  let parsed;
  try { parsed = typeof body === 'string' ? parseJson(body) : body; }
  catch {
    return `${prefix} ${typeof body === 'string' && body.includes('=')
      ? canonicalParams(new URLSearchParams(body), callerIds, options) : body}`;
  }
  parsed = options.requestBody?.(parsed, { method, url, callerIds }) ?? parsed;
  return `${prefix} ${JSON.stringify(normalize(parsed, callerIds, true, options.ignoreGeneratedBodyId
    ? { ...options, bodyContext: { method: method.toUpperCase(), url } } : options))}`;
};

export const subscriptionKey = (url, text, callerIds = new Set(), options = {}) => {
  let message;
  if (options.bingxProtocol && ['Ping', 'Pong'].includes(text)) return undefined;
  try { message = typeof text === 'string' ? JSON.parse(text) : text; }
  catch { return `WS ${canonicalUrl(url, callerIds)} ${text}`; }
  const method = String(message?.method ?? message?.reqType ?? message?.op ?? '').toLowerCase();
  if (options.bingxProtocol && (message?.ping !== undefined || message?.pong !== undefined)) return undefined;
  if (options.cryptoProtocol && method === 'public/respond-heartbeat') return undefined;
  if (options.phemexProtocol && ['server.ping', 'server.pong'].includes(method)) return undefined;
  if (['ping', 'pong', 'heartbeat'].includes(method)) return undefined;
  if (message && typeof message === 'object') {
    message = { ...message };
    delete message.id;
    delete message.req_id;
    // Phemex's independent socket audit validates these clocks and HMAC bytes.
    // Keep the API kind/key exact while comparing fresh connection signatures.
    if (options.phemexProtocol && method === 'user.auth' && Array.isArray(message.params) && message.params.length === 4) {
      message.params = [message.params[0], message.params[1], '<signature>', '<expiry>'];
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
      message.args = message.args.map((arg) => ({ ...arg, timestamp: '<timestamp>', sign: '<signature>',
        ...(options.blofinProtocol ? { nonce: '<nonce>' } : {}) }));
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
  try { message = typeof text === 'string' ? parseJson(text) : text; }
  catch {
    const key = subscriptionKey(url, text, callerIds, options);
    return key === undefined ? [] : [key];
  }
  const method = String(message?.method ?? message?.reqType ?? message?.op ?? '').toLowerCase();
  if (['subscribe', 'unsubscribe'].includes(method)) {
    if (options.cryptoProtocol && Array.isArray(message.params?.channels)) {
      return message.params.channels.map((channel) => subscriptionKey(url,
        { ...message, params: { ...message.params, channels: [channel] } }, callerIds, options));
    }
    const field = Array.isArray(message.params) ? 'params' : Array.isArray(message.args) ? 'args' : undefined;
    if (field) return message[field].map((topic) => subscriptionKey(url, { ...message, [field]: [topic] }, callerIds, options));
  }
  const key = subscriptionKey(url, message, callerIds, options);
  return key === undefined ? [] : [key];
};
