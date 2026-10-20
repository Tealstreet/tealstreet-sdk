import { isWriteRequest, selectResponse } from './requests.mjs';

const parseEcho = (text) => JSON.parse(text, (_key, value, context) =>
  typeof value === 'number' && Number.isInteger(value) && !Number.isSafeInteger(value)
    ? JSON.rawJSON(context.source) : value);

const stringify = (value) => JSON.stringify(value, (_key, item) =>
  typeof item === 'bigint' ? JSON.rawJSON(String(item)) : item);

// Reads may reuse their last snapshot; a mutation response is consumed once.
export class Responses {
  constructor(records, key, check, isWrite, checkUnusedWrites = true, impl) {
    if (impl !== undefined && !['ts', 'rust'].includes(impl)) throw new Error('Unknown replay implementation');
    this.checkUnusedWrites = checkUnusedWrites;
    this.key = key;
    this.check = check;
    this.isWrite = isWrite ?? isWriteRequest;
    this.unserved = 0;
    this.queues = new Map();
    for (const record of records.filter((r) => r.type === 'http')) {
      if (record.impl !== undefined) {
        if (!['ts', 'rust'].includes(record.impl)) throw new Error('Unknown HTTP record implementation');
        if (impl === undefined) throw new Error('Tagged HTTP records require a replay implementation');
        if (record.impl !== impl) continue;
      }
      const id = key(record.method, record.url, record.requestBody);
      if (!this.queues.has(id)) this.queues.set(id, []);
      this.queues.get(id).push(record);
    }
  }
  respond(method, url, body, commandIndex = 0, timeOrMatch = 0, timeOrHeaders, matchHeaders) {
    // Time, signed headers and pre-consumption validation are independent inputs.
    const onMatch = typeof timeOrMatch === 'function' ? timeOrMatch : undefined;
    const elapsed = onMatch ? (timeOrHeaders ?? 0) : timeOrMatch;
    const headers = onMatch ? matchHeaders : timeOrHeaders;
    const queue = this.check?.(method, url, body, headers) === false ? undefined : this.queues.get(this.key(method, url, body));
    if (!queue?.length) {
      this.unserved++;
      return { status: 599, body: '' };
    }
    const candidates = onMatch && queue ? [...queue] : queue;
    const record = selectResponse(candidates, elapsed, { commandIndex, write: this.isWrite(method, url) });
    if (!record) {
      this.unserved++;
      return { status: 599, body: '' };
    }
    if (onMatch) {
      try {
        onMatch(record);
      } catch (error) {
        this.unserved++;
        throw error;
      }
    }
    // WEEX acknowledges generated IDs from the actual request, never a fixed ID.
    let responseBody = record.body;
    if (/"\$(clientAlgoId|newClientOrderId|client_id)"/.test(responseBody)) {
      const wireBody = ArrayBuffer.isView(body) || body instanceof ArrayBuffer ? new TextDecoder().decode(body) : body;
      const sent = typeof wireBody === 'string' ? parseEcho(wireBody) : wireBody;
      responseBody = responseBody.replace(/"\$(clientAlgoId|newClientOrderId|client_id)"/g, (_, field) =>
        stringify(sent?.[field] ?? null),
      );
    }
    if (responseBody.includes('"$request:')) {
      const wireBody = ArrayBuffer.isView(body) || body instanceof ArrayBuffer ? new TextDecoder().decode(body) : body;
      const sent = typeof wireBody === 'string' ? parseEcho(wireBody) : wireBody;
      responseBody = responseBody.replace(/"\$request:([\w.]+)"/g, (_, path) =>
        stringify(path.split('.').reduce((value, key) => value?.[key], sent) ?? null));
    }
    if (onMatch && candidates.length !== queue.length) queue.splice(queue.indexOf(record), 1);
    return { status: record.status, headers: record.headers ?? '', body: responseBody };
  }
  unusedWrites() {
    if (!this.checkUnusedWrites) return 0;
    return [...this.queues.values()].flat().filter((r) => this.isWrite(r.method, r.url)).length;
  }
}
