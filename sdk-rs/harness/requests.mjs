import { parseRequestJson } from './request-json.mjs';

// Request matching keeps wire types and caller IDs; only clocks/signatures
// and generated client IDs vary between otherwise equivalent replays.
const VOLATILE = new Set(['timestamp', 'recvWindow', 'signature', 'requestId']);
const CLIENT_IDS = new Set(['clientOrderId', 'newClientOrderId', 'clientAlgoId']);
const stable =
  (callerIds, options = {}, body = false) =>
  ([name, value]) => {
    if (value === undefined || options.defaultBodyValues?.[name] === value) return false;
    if (options.ignoreGeneratedBodyId?.(name, value, callerIds, options.bodyContext)) return false;
    if (callerIds.has(value)) return true;
    if ((VOLATILE.has(name) && !(body && options.preserveBodyFields?.includes(name))) || options.generatedId?.(name, String(value))) return false;
    return !CLIENT_IDS.has(name) || Boolean(options.generatedId);
  };

const wireValue = (value, callerIds, options, key, query = false) => {
  if (!callerIds.has(value) && options.generatedBodyId?.(key, value)) return ['generated'];
  if (value === null) return ['null'];
  if (Array.isArray(value))
    return ['array', value.map((item) => wireValue(item, callerIds, options, undefined, query))];
  if (typeof value === 'object')
    return [
      'object',
      Object.entries(value)
        .filter(([name, item]) =>
          query || options.volatileBodyFields
            ? stable(callerIds, options)([name, item])
            : item !== undefined &&
              options.defaultBodyValues?.[name] !== item &&
              (callerIds.has(item) || !options.generatedId?.(name, String(item))) &&
              !options.ignoreGeneratedBodyId?.(name, item, callerIds, options.bodyContext),
        )
        .sort(([a], [b]) => a.localeCompare(b))
        .map(([name, item]) => [name, wireValue(item, callerIds, options, name, query)]),
    ];
  if (typeof value === 'bigint' || typeof value === 'number') return ['number', value.toString()];
  return [typeof value, value];
};

export const requestKey = (method, url, body, callerIds = new Set(), options = {}) => {
  if (options.requestOptions) options = { ...options, ...options.requestOptions(method.toUpperCase(), new URL(url)) };
  method = method.toUpperCase();
  body = options.wireBody?.(body, { method, url, callerIds }) ?? body;
  const parsed = new URL(options.requestUrl?.(url, method) ?? url);
  const query = [...parsed.searchParams]
    .filter(
      ([name, value]) =>
        stable(callerIds, options)([name, value]) && !options.volatileQueryFields?.includes(name) &&
        (callerIds.has(value) || !options.generatedQueryId?.(name, value)) &&
        !(value === '' && options.cacheBustPaths?.includes(parsed.pathname)),
    )
    .map(([name, value]) => {
      if (/^[\[{]/.test(value)) {
        try {
          return [name, JSON.stringify(wireValue(parseRequestJson(value), callerIds, options, undefined, true))];
        } catch {
          /* Non-JSON query strings remain exact. */
        }
      }
      return [name, value];
    })
    .sort(([a], [b]) => a.localeCompare(b));
  if (ArrayBuffer.isView(body) || body instanceof ArrayBuffer) body = new TextDecoder().decode(body);
  if (body instanceof URLSearchParams || (typeof body === 'string' && !/^[\[{]/.test(body.trimStart()) && body.includes('='))) {
    const form = new URLSearchParams(body);
    const entries = [...form].filter(stable(callerIds, options, true)).sort(([a], [b]) => a.localeCompare(b));
    if (!options.requestBody || new Set([...form.keys()]).size !== [...form.keys()].length)
      return `${method} ${parsed.origin}${parsed.pathname}?${new URLSearchParams(query)} FORM ${new URLSearchParams(entries)}`;
    body = Object.fromEntries(form);
  }
  let payload = typeof body === 'string' && body ? parseRequestJson(body) : (body === '' ? {} : (body ?? {}));
  if (options.requestBody) payload = options.requestBody(payload, { method, url, callerIds });
  const hyperliquid = /(^|\.)hyperliquid(?:-testnet)?\.xyz$/.test(parsed.hostname);
  if (hyperliquid && payload?.action) {
    const { nonce: _nonce, signature: _signature, ...action } = payload;
    payload = action;
    const priorGenerated = options.generatedId;
    options = { ...options, generatedId: (key, value) => (key === 'c' && /^0x[0-9a-fA-F]{32}$/.test(value)) || Boolean(priorGenerated?.(key, value)) };
  }
  if (options.ignoreGeneratedBodyId) options = { ...options, bodyContext: { method, url } };
  const data =
    payload !== null && typeof payload === 'object' && !Array.isArray(payload)
      ? [
          'object',
          Object.entries(payload)
            .filter(stable(callerIds, options, true))
            .sort(([a], [b]) => a.localeCompare(b))
            .map(([key, value]) => [key, wireValue(value, callerIds, options, key)]),
        ]
      : wireValue(payload, callerIds, options);
  return `${method} ${parsed.origin}${parsed.pathname}?${new URLSearchParams(query)} ${JSON.stringify(data)}`;
};

export const isRead = (method, url) => !isWriteRequest(method, url);

export const takeResponse = (queue, method, url, time = 0, commandIndex = Infinity, repeat = isRead(method, url)) => {
  if (!queue?.length) return undefined;
  const eligible = queue.filter(
    (r) => (r.availableAfter ?? r.effectiveAt ?? 0) <= time && (r.afterCommand ?? 0) <= commandIndex,
  );
  if (!eligible.length) return undefined;
  const stage = Math.max(...eligible.map((r) => r.afterCommand ?? 0));
  const current = eligible.filter((r) => (r.afterCommand ?? 0) === stage);
  if (repeat && current.some((r) => r.availableAfter !== undefined || r.effectiveAt !== undefined))
    return current.at(-1);
  const record = current[0];
  if (current.length > 1 || !repeat) queue.splice(queue.indexOf(record), 1);
  return record;
};

/** Subscription IDs and batching vary; native channels and operations must match. */
const legacySubscriptionKeys = (url, text) => {
  let message;
  try {
    message = JSON.parse(text);
  } catch {
    return [];
  }
  const method = String(message.method ?? message.reqType ?? message.op ?? '').toUpperCase();
  if (!['SUBSCRIBE', 'UNSUBSCRIBE', 'SUB', 'UNSUB'].includes(method)) return [];
  const op = ['SUBSCRIBE', 'SUB'].includes(method) ? 'subscribe' : 'unsubscribe';
  const parsed = new URL(url);
  const topics =
    message.params?.channels ?? message.params ?? message.args ?? (message.dataType ? [message.dataType] : []);
  if (!Array.isArray(topics)) return [];
  return topics.map(
    (topic) =>
      `${parsed.origin}${parsed.pathname} ${op} ${typeof topic === 'object' ? JSON.stringify(Object.fromEntries(Object.entries(topic).sort(([a], [b]) => a.localeCompare(b)))) : topic}`,
  );
};

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
  const at = (record) => Math.max(record.availableAt ?? 0, record.effectiveAt ?? 0, record.availableAfter ?? 0, record.after ?? 0);
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


const IDENTITY_CLOCKS = new Set(['timestamp', 'recvWindow', 'signature', 'requestId']);
const IDENTITY_URL_VOLATILE = new Set(IDENTITY_CLOCKS);
const IDENTITY_CLIENT_IDS = new Set(['newClientOrderId', 'clientAlgoId', 'clientOrderId', 'c']);
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
  if (typeof value === 'bigint') return JSON.rawJSON(String(value));
  if (JSON.isRawJSON(value)) return value;
  if (Array.isArray(value)) return value.map((item) => normalize(item, callerIds, false, options));
  if (value && typeof value === 'object') return Object.fromEntries(Object.keys(value).sort()
    .filter((key) => !(root && (IDENTITY_CLOCKS.has(key) || (key === 'nonce' && value.action))))
    .filter((key) => options.defaultBodyValues?.[key] !== value[key])
    .filter((key) => !options.ignoreGeneratedBodyId?.(key, value[key], callerIds, options.bodyContext))
    .filter((key) => !IDENTITY_CLIENT_IDS.has(key) || callerIds.has(value[key])
      || (options.generatedId && !options.generatedId(key, String(value[key]))))
    .map((key) => [key, !callerIds.has(value[key]) && options.generatedBodyId?.(key, value[key])
      ? '<generated>' : normalize(value[key], callerIds, false, options)]));
  return value;
};

const canonicalParams = (params, callerIds, options = {}) => new URLSearchParams([...params]
  .filter(([key]) => !IDENTITY_URL_VOLATILE.has(key) && !options.volatileQueryFields?.includes(key))
  .filter(([key, value]) => !IDENTITY_CLIENT_IDS.has(key) || callerIds.has(value))
  .map(([key, value]) => {
    if (IDENTITY_CLIENT_IDS.has(key)) return [key, value];
    if (!/^[\[{]/.test(value.trimStart())) return [key, value];
    try { return [key, JSON.stringify(normalize(parseJson(value), callerIds))]; }
    catch { return [key, value]; }
  })
  .sort(([a, av], [b, bv]) => a.localeCompare(b) || av.localeCompare(bv))).toString();

export const canonicalUrl = (url, callerIds = new Set(), options = {}) => {
  const parsed = new URL(url);
  return `${parsed.origin}${parsed.pathname}?${canonicalParams(parsed.searchParams, callerIds, options)}`;
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
  try { message = typeof text === 'string' ? JSON.parse(text) : text; }
  catch { return `WS ${canonicalUrl(url, callerIds, options)} ${text}`; }
  if (options.socketMessage) {
    message = options.socketMessage(message);
    if (message === undefined) return undefined;
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
  if (options.phemexProtocol && ['server.ping','server.pong'].includes(method)) return undefined;
  if (['ping', 'pong', 'heartbeat'].includes(method)) return undefined;
  if (message && typeof message === 'object') {
    message = { ...message };
    delete message.id;
    delete message.req_id;
    if (options.rust === 'kucoin') delete message.response;
    if (options.phemexProtocol && method === 'user.auth' && Array.isArray(message.params) && message.params.length === 4)
      message.params = [message.params[0], message.params[1], '<signature>', '<expiry>'];
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
      message.args = message.args.map((arg) => {
        const nested = typeof arg === 'string';
        const payload = nested ? parseJson(arg) : arg;
        const normalized = { ...payload, timestamp: '<timestamp>', sign: '<signature>',
          ...(nested && 'signature' in payload ? { signature: '<signature>' } : {}),
          ...(options.blofinProtocol ? { nonce: '<nonce>' } : {}) };
        return nested ? JSON.stringify(normalized) : normalized;
      });
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
  return `WS ${canonicalUrl(url, callerIds, options)} ${JSON.stringify(normalize(message, callerIds))}`;
};

// Subscription batches and one-topic sends have the same required feed set.
const fullSubscriptionKeys = (url, text, callerIds = new Set(), options = {}) => {
  let message;
  try { message = typeof text === 'string' ? parseJson(text) : text; }
  catch {
    const key = subscriptionKey(url, text, callerIds, options);
    return key === undefined ? [] : [key];
  }
  const method = String(message?.method ?? message?.reqType ?? message?.op ?? message?.event ?? message?.type ?? '').toLowerCase();
  if (['subscribe', 'unsubscribe'].includes(method)) {
    if (options.rust === 'kucoin' && typeof message.topic === 'string') {
      const colon = message.topic.indexOf(':');
      if (colon >= 0) return message.topic.slice(colon + 1).split(',').map((symbol) =>
        subscriptionKey(url, { ...message, topic: `${message.topic.slice(0, colon)}:${symbol}` }, callerIds, options));
    }
    if (options.cryptoProtocol && Array.isArray(message.params?.channels)) {
      return message.params.channels.map((channel) => subscriptionKey(url,
        { ...message, params: { ...message.params, channels: [channel] } }, callerIds, options));
    }
    if (options.productSubscriptions && Array.isArray(message.product_ids)) return message.product_ids.map((product) =>
      subscriptionKey(url, { ...message, product_ids: [product] }, callerIds, options));
    const field = Array.isArray(message.params) ? 'params' : Array.isArray(message.args) ? 'args' : undefined;
    if (field) return message[field].map((topic) => subscriptionKey(url, { ...message, [field]: [topic] }, callerIds, options));
  }
  const key = subscriptionKey(url, message, callerIds, options);
  return key === undefined ? [] : [key];
};

export const subscriptionKeys = (url, text, callerIds = new Set(), options = {}) => {
  if (['Ping', 'Pong', 'ping', 'pong'].includes(text)) return [];
  if (typeof text === 'string') { try { const m = JSON.parse(text); if (m?.ping !== undefined || m?.pong !== undefined) return []; } catch {} }
  const legacy = legacySubscriptionKeys(url, typeof text === 'string' ? text : JSON.stringify(text));
  return options.cryptoProtocol || options.bingxProtocol || options.mexcProtocol || options.blofinProtocol ||
    options.fullSocketIdentity || options.bitrueProtocol || !legacy.length
    ? fullSubscriptionKeys(url, text, callerIds, options) : legacy;
};
