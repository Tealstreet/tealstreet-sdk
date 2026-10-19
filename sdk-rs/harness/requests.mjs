import { parseRequestJson } from './request-json.mjs';

// Request matching keeps wire types and caller IDs; only clocks/signatures
// and generated client IDs vary between otherwise equivalent replays.
const VOLATILE = new Set(['timestamp', 'recvWindow', 'signature', 'requestId']);
const CLIENT_IDS = new Set(['clientOrderId', 'newClientOrderId', 'clientAlgoId']);
const stable = (callerIds) => ([name, value]) =>
  !VOLATILE.has(name) && !(CLIENT_IDS.has(name) && !callerIds.has(value));

const wireValue = (value, callerIds, generatedBodyId, key) => {
  if (!callerIds.has(value) && generatedBodyId?.(key, value)) return ['generated'];
  if (value === null) return ['null'];
  if (Array.isArray(value)) return ['array', value.map((item) => wireValue(item, callerIds, generatedBodyId))];
  if (typeof value === 'object') return ['object', Object.entries(value).sort(([a],[b]) => a.localeCompare(b)).map(([key,value]) => [key,wireValue(value, callerIds, generatedBodyId, key)])];
  if (typeof value === 'bigint' || typeof value === 'number') return ['number', value.toString()];
  return [typeof value, value];
};

export const requestKey = (method, url, body, callerIds = new Set(), { generatedBodyId, requestBody } = {}) => {
  const parsed = new URL(url);
  const query = [...parsed.searchParams].filter(stable(callerIds))
    .map(([name, value]) => [name, value.replace(/"(newClientOrderId|clientAlgoId)":"([^"]*)",?/g,
      (match, _field, id) => callerIds.has(id) ? match : '')])
    .sort(([a], [b]) => a.localeCompare(b));
  if (ArrayBuffer.isView(body) || body instanceof ArrayBuffer) body = new TextDecoder().decode(body);
  let payload = typeof body === 'string' && body ? parseRequestJson(body) : body ?? {};
  if (requestBody) payload = requestBody(payload);
  const data = payload !== null && typeof payload === 'object' && !Array.isArray(payload)
    ? ['object', Object.entries(payload).filter(stable(callerIds)).sort(([a], [b]) => a.localeCompare(b)).map(([key,value]) => [key,wireValue(value, callerIds, generatedBodyId, key)])]
    : wireValue(payload, callerIds, generatedBodyId);
  return `${method} ${parsed.origin}${parsed.pathname}?${new URLSearchParams(query)} ${JSON.stringify(data)}`;
};

export const isRead = (method, url) => method === 'GET' || (['POST', 'PUT', 'DELETE'].includes(method) && /\/(listenKey|userDataStream)$/.test(new URL(url).pathname));

export const takeResponse = (queue, method, url, time = 0, commandIndex = Infinity, repeat = isRead(method, url)) => {
  if (!queue?.length) return undefined;
  const eligible = queue.filter((r) => (r.availableAfter ?? 0) <= time && (r.afterCommand ?? 0) <= commandIndex);
  if (!eligible.length) return undefined;
  const stage = Math.max(...eligible.map((r) => r.afterCommand ?? 0));
  const current = eligible.filter((r) => (r.afterCommand ?? 0) === stage);
  if (repeat && current.some((r) => r.availableAfter !== undefined)) return current.at(-1);
  const record = current[0];
  if (current.length > 1 || !repeat) queue.splice(queue.indexOf(record), 1);
  return record;
};

/** Subscription IDs and batching vary; native channels and operations must match. */
export const subscriptionKeys = (url, text) => {
  let message;
  try { message = JSON.parse(text); } catch { return []; }
  const method = String(message.method ?? message.reqType ?? message.op ?? message.event ?? '').toUpperCase();
  if (!['SUBSCRIBE', 'UNSUBSCRIBE', 'SUB', 'UNSUB'].includes(method)) return [];
  const op = ['SUBSCRIBE', 'SUB'].includes(method) ? 'subscribe' : 'unsubscribe';
  const parsed = new URL(url);
  const topics = message.params?.channels ?? message.params ?? message.args ?? (message.dataType ? [message.dataType] : typeof message.topic === 'string' ? [message.topic] : []);
  if (!Array.isArray(topics)) return [];
  return topics.map((topic) => `${parsed.origin}${parsed.pathname} ${op} ${typeof topic === 'object' ? JSON.stringify(Object.fromEntries(Object.entries(topic).sort(([a],[b]) => a.localeCompare(b)))) : topic}`);
};
