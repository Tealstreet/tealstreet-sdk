import { isRead, takeResponse } from './requests.mjs';
import { parseRequestJson } from './request-json.mjs';

// Reads may reuse their last snapshot; a mutation response is consumed once.
export class Responses {
  constructor(records, key, check, isWrite = (method, url) => !isRead(method, url), checkUnusedWrites = true, impl) {
    if (impl !== undefined && !['ts', 'rust'].includes(impl)) throw new Error('Unknown replay implementation');
    this.key = key;
    this.check = check;
    this.isWrite = isWrite ?? ((method, url) => !isRead(method, url));
    this.checkUnusedWrites = checkUnusedWrites;
    this.unserved = 0;
    this.queues = new Map();
    for (const original of records.filter((r) => r.type === 'http')) {
      if (original.impl !== undefined) {
        if (!['ts', 'rust'].includes(original.impl)) throw new Error('Unknown HTTP record implementation');
        if (impl === undefined) throw new Error('Tagged HTTP records require a replay implementation');
        if (original.impl !== impl) continue;
      }
      // Both tape spellings guard visibility; callers of the factory need not
      // normalize records through replay.mjs first.
      const record =
        original.effectiveAt !== undefined && original.availableAfter === undefined
          ? { ...original, availableAfter: original.effectiveAt }
          : original;
      const id = key(record.method, record.url, record.requestBody);
      if (!this.queues.has(id)) this.queues.set(id, []);
      this.queues.get(id).push(record);
    }
  }
  respond(method, url, body, commandIndex = 0, timeOrMatch = 0, matchTime = 0) {
    // Preserve the incoming time argument and the factory's pre-consumption
    // validation hook. A rejected generated ID must leave its write available.
    const onMatch = typeof timeOrMatch === 'function' ? timeOrMatch : undefined;
    const time = onMatch ? matchTime : timeOrMatch;
    const queue = this.check?.(method, url, body) === false ? undefined : this.queues.get(this.key(method, url, body));
    const candidates = onMatch && queue ? [...queue] : queue;
    const record = takeResponse(candidates, method, url, time, commandIndex, !this.isWrite(method, url));
    if (!record) {
      this.unserved++;
      return { status: 599, body: '' };
    }
    onMatch?.(record);
    let responseBody = record.body;
    if (typeof responseBody === 'string' && /"\$(clientAlgoId|newClientOrderId)"/.test(responseBody)) {
      const sent = !body
        ? {}
        : typeof body === 'string'
          ? parseRequestJson(body)
          : ArrayBuffer.isView(body) || body instanceof ArrayBuffer
            ? parseRequestJson(new TextDecoder().decode(body))
            : body;
      responseBody = responseBody.replace(/"\$(clientAlgoId|newClientOrderId)"/g, (_match, field) =>
        JSON.stringify(sent[field] ?? null),
      );
    }
    if (onMatch && candidates.length !== queue.length) queue.splice(queue.indexOf(record), 1);
    return { status: record.status, headers: record.headers ?? '', body: responseBody };
  }
  unusedWrites() {
    if (!this.checkUnusedWrites) return 0;
    return [...this.queues.values()].flat().filter((r) => this.isWrite(r.method, r.url)).length;
  }
}
