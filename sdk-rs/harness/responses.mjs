// Reads may reuse their last snapshot; a mutation response is consumed once.
import { parseRequestJson } from './request-json.mjs';

export class Responses {
  constructor(records, key, check, isWrite) {
    this.key = key;
    this.check = check;
    this.isWrite = isWrite;
    this.unserved = 0;
    this.queues = new Map();
    for (const record of records.filter((r) => r.type === 'http')) {
      const id = key(record.method, record.url, record.requestBody);
      if (!this.queues.has(id)) this.queues.set(id, []);
      this.queues.get(id).push(record);
    }
  }
  respond(method, url, body, commandIndex = 0, onMatch, time = 0) {
    const queue = this.check?.(method, url, body) === false ? undefined : this.queues.get(this.key(method, url, body));
    if (!queue?.length) {
      this.unserved++;
      return { status: 599, body: '' };
    }
    const eligible = queue.filter((r) => (r.afterCommand ?? 0) <= commandIndex);
    if (!eligible.length) {
      this.unserved++;
      return { status: 599, body: '' };
    }
    const stage = Math.max(...eligible.map((r) => r.afterCommand ?? 0));
    const current = eligible.filter((r) => (r.afterCommand ?? 0) === stage);
    const write = this.isWrite?.(method, url);
    const timedRead = !write && current.some((record) => record.effectiveAt !== undefined);
    const timed = current.filter((record) => record.effectiveAt !== undefined && record.effectiveAt <= time);
    const record = timedRead ? (timed.at(-1) ?? current[0]) : current[0];
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
    if (!timedRead && (current.length > 1 || write)) queue.splice(queue.indexOf(record), 1);
    return { status: record.status, headers: record.headers ?? '', body: responseBody };
  }
  unusedWrites() {
    return [...this.queues.values()].flat().filter((r) => this.isWrite?.(r.method, r.url)).length;
  }
}
