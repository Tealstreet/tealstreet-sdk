import { parseRequestJson } from './request-json.mjs';
import { isRead } from './requests.mjs';

// Reads may reuse their last snapshot; a mutation response is consumed once.
export class Responses {
  constructor(records, key, check, isWrite = (method, url) => !isRead(method, new URL(url, 'https://replay.invalid').href), fallbackKey) {
    this.key = key;
    this.check = check;
    this.isWrite = isWrite;
    this.fallbackKey = fallbackKey;
    this.unserved = 0;
    this.queues = new Map();
    for (const record of records.filter((r) => r.type === 'http')) {
      const id = key(record.method, record.url, record.requestBody);
      if (!this.queues.has(id)) this.queues.set(id, []);
      this.queues.get(id).push(record);
    }
  }
  respond(method, url, body, commandIndex = 0, time = 0) {
    const queue = this.check?.(method, url, body) === false ? undefined :
      this.queues.get(this.key(method, url, body)) ??
      (this.fallbackKey && this.queues.get(this.fallbackKey(method, url, body)));
    if (!queue?.length) {
      this.unserved++;
      return { status: 599, body: '' };
    }
    const eligible = queue.filter((r) => (r.afterCommand ?? 0) <= commandIndex && (r.after ?? 0) <= time && (r.effectiveAt ?? 0) <= time && (r.availableAfter ?? 0) <= time);
    if (!eligible.length) {
      this.unserved++;
      return { status: 599, body: '' };
    }
    const stage = Math.max(...eligible.map((r) => r.afterCommand ?? 0));
    let current = eligible.filter((r) => (r.afterCommand ?? 0) === stage);
    const write = this.isWrite?.(method, url) ?? method !== 'GET';
    const timed = !write && queue.some((r) => r.after !== undefined || r.effectiveAt !== undefined || r.availableAfter !== undefined);
    if (timed) {
      const after = Math.max(...current.map((r) => Math.max(r.after ?? 0, r.effectiveAt ?? 0, r.availableAfter ?? 0)));
      current = current.filter((r) => Math.max(r.after ?? 0, r.effectiveAt ?? 0, r.availableAfter ?? 0) === after);
    }
    const record = timed ? current.at(-1) : current[0];
    if ((!timed && current.length > 1) || write) queue.splice(queue.indexOf(record), 1);
    let responseBody = record.body;
    if (/"\$(clientAlgoId|newClientOrderId)"/.test(responseBody)) {
      const raw = ArrayBuffer.isView(body) || body instanceof ArrayBuffer ? new TextDecoder().decode(body) : body;
      const sent = typeof raw === 'string' ? parseRequestJson(raw) : raw ?? {};
      responseBody = responseBody.replace(/"\$(clientAlgoId|newClientOrderId)"/g,
        (_, field) => JSON.stringify(sent[field] ?? null));
    }
    return { status: record.status, headers: record.headers ?? '', body: responseBody };
  }
  unusedWrites() {
    return [...this.queues.values()].flat().filter((r) => (this.isWrite?.(r.method, r.url) ?? r.method !== 'GET')).length;
  }
}
