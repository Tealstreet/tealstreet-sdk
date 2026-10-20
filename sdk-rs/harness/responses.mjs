import { isWriteRequest, selectResponse } from './requests.mjs';
import { parseRequestJson } from './request-json.mjs';

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
    for (const queue of this.queues.values()) {
      if (!queue.some((record) => record.requestOrdinal !== undefined)) continue;
      const ordinals = queue.map((record) => record.requestOrdinal);
      if (ordinals.some((ordinal) => !Number.isSafeInteger(ordinal) || ordinal < 0)
          || new Set(ordinals).size !== queue.length) {
        throw new Error('HTTP request ordinals must be unique nonnegative integers within each queue');
      }
      queue.sort((a, b) => a.requestOrdinal - b.requestOrdinal);
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
    // Acknowledgements echo generated IDs from the matched request, never a fixed ID.
    let responseBody = record.body;
    if (/"\$(clientAlgoId|newClientOrderId|client_id)"/.test(responseBody)) {
      const wireBody = ArrayBuffer.isView(body) || body instanceof ArrayBuffer ? new TextDecoder().decode(body) : body;
      const sent = typeof wireBody === 'string' ? parseRequestJson(wireBody) : wireBody;
      responseBody = responseBody.replace(/"\$(clientAlgoId|newClientOrderId|client_id)"/g, (_, field) =>
        JSON.stringify(sent?.[field] ?? new URL(url).searchParams.get(field), (_key, value) =>
          typeof value === 'bigint' ? JSON.rawJSON(String(value)) : value,
        ),
      );
    }
    if (responseBody.includes('"$request:')) {
      const wireBody = ArrayBuffer.isView(body) || body instanceof ArrayBuffer ? new TextDecoder().decode(body) : body;
      const sent = typeof wireBody === 'string' ? parseRequestJson(wireBody) : wireBody;
      responseBody = responseBody.replace(/"\$request:([\w.]+)"/g, (_, path) =>
        JSON.stringify(path.split('.').reduce((value, key) => value?.[key], sent) ?? null));
    }
    if (onMatch && candidates.length !== queue.length) queue.splice(queue.indexOf(record), 1);
    this.served = record;
    return { status: record.status, headers: record.headers ?? '', body: responseBody };
  }
  unusedWrites() {
    if (!this.checkUnusedWrites) return 0;
    return [...this.queues.values()].flat().filter((r) => this.isWrite(r.method, r.url)).length;
  }
}
