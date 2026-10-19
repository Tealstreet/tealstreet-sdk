// Stable request identity. Caller IDs remain exact; generated IDs and transport
// signatures/clocks are nondeterministic. Economic action fields stay intact.
const CLOCKS = new Set(['timestamp', 'recvWindow', 'signature']);
const CLIENT_IDS = new Set(['newClientOrderId', 'clientAlgoId', 'clientOrderId', 'c']);
const parseJson = (text) => JSON.parse(text, (_key, value, context) =>
  typeof value === 'number' && Number.isInteger(value) && !Number.isSafeInteger(value)
    ? JSON.rawJSON(context.source) : value);

export const callerIdsOf = (records) => new Set(records
  .filter((record) => record.type === 'command')
  .flatMap((record) => (record.orders ?? []).map((order) => order.clientOrderId))
  .filter((id) => typeof id === 'string'));

const normalize = (value, callerIds, root = true) => {
  if (JSON.isRawJSON(value)) return value;
  if (Array.isArray(value)) return value.map((item) => normalize(item, callerIds, false));
  if (value && typeof value === 'object') return Object.fromEntries(Object.keys(value).sort()
    .filter((key) => !(root && (CLOCKS.has(key) || (key === 'nonce' && value.action))))
    .filter((key) => !CLIENT_IDS.has(key) || callerIds.has(value[key]))
    .map((key) => [key, normalize(value[key], callerIds, false)]));
  return value;
};

const canonicalParams = (params, callerIds) => new URLSearchParams([...params]
  .filter(([key]) => !CLOCKS.has(key))
  .filter(([key, value]) => !CLIENT_IDS.has(key) || callerIds.has(value))
  .map(([key, value]) => {
    if (CLIENT_IDS.has(key)) return [key, value];
    if (!/^[\[{]/.test(value.trimStart())) return [key, value];
    try { return [key, JSON.stringify(normalize(parseJson(value), callerIds))]; }
    catch { return [key, value]; }
  })
  .sort(([a, av], [b, bv]) => a.localeCompare(b) || av.localeCompare(bv))).toString();

export const canonicalUrl = (url, callerIds = new Set()) => {
  const parsed = new URL(url);
  return `${parsed.origin}${parsed.pathname}?${canonicalParams(parsed.searchParams, callerIds)}`;
};

export const requestKey = (method, url, body, callerIds = new Set()) => {
  const prefix = `${method.toUpperCase()} ${canonicalUrl(url, callerIds)}`;
  if (body == null || body === '') return prefix;
  if (body instanceof URLSearchParams) return `${prefix} ${canonicalParams(body, callerIds)}`;
  let parsed;
  try { parsed = typeof body === 'string' ? parseJson(body) : body; }
  catch {
    return `${prefix} ${typeof body === 'string' && body.includes('=')
      ? canonicalParams(new URLSearchParams(body), callerIds) : body}`;
  }
  return `${prefix} ${JSON.stringify(normalize(parsed, callerIds))}`;
};

export const subscriptionKey = (url, text, callerIds = new Set()) => {
  let message;
  try { message = typeof text === 'string' ? JSON.parse(text) : text; }
  catch { return `WS ${canonicalUrl(url, callerIds)} ${text}`; }
  const method = String(message?.method ?? message?.op ?? '').toLowerCase();
  if (['ping', 'pong', 'heartbeat'].includes(method)) return undefined;
  if (message && typeof message === 'object') {
    message = { ...message };
    delete message.id;
    if (['subscribe', 'unsubscribe'].includes(method) && Array.isArray(message.params)) {
      message.params = [...message.params].sort();
    }
  }
  return `WS ${canonicalUrl(url, callerIds)} ${JSON.stringify(normalize(message, callerIds))}`;
};

// Subscription batches and one-topic sends have the same required feed set.
export const subscriptionKeys = (url, text, callerIds = new Set()) => {
  let message;
  try { message = typeof text === 'string' ? parseJson(text) : text; }
  catch { return [subscriptionKey(url, text, callerIds)]; }
  const method = String(message?.method ?? message?.op ?? '').toLowerCase();
  if (['subscribe', 'unsubscribe'].includes(method) && Array.isArray(message.params)) {
    return message.params.map((param) => subscriptionKey(url, { ...message, params: [param] }, callerIds));
  }
  const key = subscriptionKey(url, message, callerIds);
  return key === undefined ? [] : [key];
};
