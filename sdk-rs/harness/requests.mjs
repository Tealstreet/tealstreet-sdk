import { parseRequestJson } from './request-json.mjs';

// Request matching keeps wire types and caller IDs; only clocks/signatures
// and generated client IDs vary between otherwise equivalent replays.
const VOLATILE = new Set(['timestamp', 'recvWindow', 'signature', 'requestId']);
const CLIENT_IDS = new Set(['clientOrderId', 'newClientOrderId', 'clientAlgoId']);
const stable =
  (callerIds, options = {}) =>
  ([name, value]) => {
    if (value === undefined || options.defaultBodyValues?.[name] === value) return false;
    if (callerIds.has(value)) return true;
    if (VOLATILE.has(name) || options.generatedId?.(name, String(value))) return false;
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
              (callerIds.has(item) || !options.generatedId?.(name, String(item))),
        )
        .sort(([a], [b]) => a.localeCompare(b))
        .map(([name, item]) => [name, wireValue(item, callerIds, options, name, query)]),
    ];
  if (typeof value === 'bigint' || typeof value === 'number') return ['number', value.toString()];
  return [typeof value, value];
};

export const requestKey = (method, url, body, callerIds = new Set(), options = {}) => {
  const parsed = new URL(options.requestUrl?.(url) ?? url);
  const query = [...parsed.searchParams]
    .filter(
      ([name, value]) =>
        stable(callerIds, options)([name, value]) &&
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
  let payload = typeof body === 'string' && body ? parseRequestJson(body) : (body ?? {});
  if (options.requestBody) payload = options.requestBody(payload);
  const data =
    payload !== null && typeof payload === 'object' && !Array.isArray(payload)
      ? [
          'object',
          Object.entries(payload)
            .filter(stable(callerIds, options))
            .sort(([a], [b]) => a.localeCompare(b))
            .map(([key, value]) => [key, wireValue(value, callerIds, options, key)]),
        ]
      : wireValue(payload, callerIds, options);
  return `${method} ${parsed.origin}${parsed.pathname}?${new URLSearchParams(query)} ${JSON.stringify(data)}`;
};

export const isRead = (method, url) =>
  method === 'GET' ||
  (['POST', 'PUT', 'DELETE'].includes(method) && /\/(listenKey|userDataStream)$/.test(new URL(url).pathname));

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
