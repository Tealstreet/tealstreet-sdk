// Request matching keeps wire types and caller IDs; only clocks/signatures
// and generated client IDs vary between otherwise equivalent replays.
const VOLATILE = new Set(['timestamp', 'recvWindow', 'signature', 'requestId']);
const CLIENT_IDS = new Set(['clientOrderId', 'newClientOrderId', 'clientAlgoId']);
const stable = (callerIds) => ([name, value]) =>
  !VOLATILE.has(name) && !(CLIENT_IDS.has(name) && !callerIds.has(value));

const wireValue = (value) => {
  if (value === null) return ['null'];
  if (Array.isArray(value)) return ['array', value.map(wireValue)];
  if (typeof value === 'object') return ['object', Object.entries(value).sort(([a],[b]) => a.localeCompare(b)).map(([key,value]) => [key,wireValue(value)])];
  if (typeof value === 'bigint' || typeof value === 'number') return ['number', value.toString()];
  return [typeof value, value];
};

export const requestKey = (method, url, body, callerIds = new Set()) => {
  const parsed = new URL(url);
  const query = [...parsed.searchParams].filter(stable(callerIds))
    .map(([name, value]) => [name, value.replace(/"(newClientOrderId|clientAlgoId)":"([^"]*)",?/g,
      (match, _field, id) => callerIds.has(id) ? match : '')])
    .sort(([a], [b]) => a.localeCompare(b));
  if (body instanceof Uint8Array) body = new TextDecoder().decode(body);
  const payload = typeof body === 'string' && body ? JSON.parse(body, (_key,value,context) =>
    typeof value === 'number' && !Number.isSafeInteger(value) && context?.source && /^-?\d+$/.test(context.source) ? BigInt(context.source) : value) : body ?? {};
  const data = Object.entries(payload).filter(stable(callerIds)).sort(([a], [b]) => a.localeCompare(b)).map(([key,value]) => [key,wireValue(value)]);
  return `${method} ${parsed.origin}${parsed.pathname}?${new URLSearchParams(query)} ${JSON.stringify(data)}`;
};

export const takeResponse = (queue, method, url, time = 0) => {
  if (!queue?.length) return undefined;
  const repeat = method === 'GET' || (['POST', 'PUT'].includes(method) && /\/(listenKey|userDataStream)$/.test(new URL(url).pathname));
  if (repeat && queue.some((r) => r.availableAfter !== undefined)) {
    return queue.findLast((r) => (r.availableAfter ?? 0) <= time) ?? queue[0];
  }
  return queue.length > 1 || !repeat ? queue.shift() : queue[0];
};

/** Subscription IDs and batching vary; native channels and operations must match. */
export const subscriptionKeys = (url, text) => {
  let message;
  try { message = JSON.parse(text); } catch { return []; }
  const method = String(message.method ?? message.reqType ?? message.op ?? '').toUpperCase();
  if (!['SUBSCRIBE', 'UNSUBSCRIBE', 'SUB', 'UNSUB'].includes(method)) return [];
  const op = ['SUBSCRIBE', 'SUB'].includes(method) ? 'subscribe' : 'unsubscribe';
  const parsed = new URL(url);
  const topics = message.params ?? message.args ?? (message.dataType ? [message.dataType] : []);
  return topics.map((topic) => `${parsed.origin}${parsed.pathname} ${op} ${topic}`);
};
