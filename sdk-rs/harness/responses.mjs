import { parseRequestJson } from './request-json.mjs';
import { isRead } from './requests.mjs';

export const implementationRecords = (records, impl) => {
  if (impl !== undefined && !['ts', 'rust'].includes(impl)) throw new Error('Unknown replay implementation');
  return records.filter((record) => {
    if (record.impl === undefined) return true;
    if (!['ts', 'rust'].includes(record.impl)) throw new Error(`Unknown ${record.type === 'http' ? 'HTTP' : 'tape'} record implementation`);
    if (impl === undefined) throw new Error(`Tagged ${record.type === 'http' ? 'HTTP' : 'tape'} records require a replay implementation`);
    return record.impl === impl;
  });
};

// Reads may reuse their last snapshot; a mutation response is consumed once.
export class Responses {
  constructor(records, key, check, isWrite = (method, url) => !isRead(method, new URL(url, 'https://replay.invalid').href), fallbackKey, impl) {
    records = implementationRecords(records, impl);
    this.key = key;
    this.check = check;
    this.isWrite = typeof isWrite === 'function' ? isWrite : (method, url) => !isRead(method, new URL(url, 'https://replay.invalid').href);
    this.fallbackKey = fallbackKey;
    this.unserved = 0;
    this.queues = new Map();
    for (const record of records.filter((r) => r.type === 'http')) {
      const id = key(record.method, record.url, record.requestBody);
      if (!this.queues.has(id)) this.queues.set(id, []);
      this.queues.get(id).push(record);
    }
  }
  respond(method, url, body, commandIndex = 0, timeOrMatch = 0, matchOrHeaders, signedHeaders) {
    const legacyMatch = typeof timeOrMatch === 'function';
    const onMatch = legacyMatch ? timeOrMatch : typeof matchOrHeaders === 'function' ? matchOrHeaders : undefined;
    const time = legacyMatch ? (matchOrHeaders ?? 0) : timeOrMatch;
    const headers = legacyMatch || onMatch ? signedHeaders : matchOrHeaders ?? signedHeaders;
    const queue = this.check?.(method, url, body, headers) === false ? undefined :
      this.queues.get(this.key(method, url, body)) ??
      (this.fallbackKey && this.queues.get(this.fallbackKey(method, url, body)));
    if (!queue?.length) {
      this.unserved++;
      return { status: 599, body: '' };
    }
    const write = this.isWrite?.(method, url) ?? method !== 'GET';
    if (write && ((queue[0].afterCommand ?? 0) > commandIndex || Math.max(queue[0].after ?? 0, queue[0].effectiveAt ?? 0, queue[0].availableAfter ?? 0, queue[0].availableAt ?? 0) > time)) {
      this.unserved++;
      return { status: 599, body: '' };
    }
    const eligible = queue.filter((r) => (r.afterCommand ?? 0) <= commandIndex && (r.after ?? 0) <= time && (r.effectiveAt ?? 0) <= time && (r.availableAfter ?? 0) <= time && (r.availableAt ?? 0) <= time);
    if (!eligible.length) {
      this.unserved++;
      return { status: 599, body: '' };
    }
    const stage = Math.max(...eligible.map((r) => r.afterCommand ?? 0));
    let current = eligible.filter((r) => (r.afterCommand ?? 0) === stage);
    const timed = !write && current.some((r) => r.after !== undefined || r.effectiveAt !== undefined || r.availableAfter !== undefined || r.availableAt !== undefined);
    if (timed) {
      const after = Math.max(...current.map((r) => Math.max(r.after ?? 0, r.effectiveAt ?? 0, r.availableAfter ?? 0, r.availableAt ?? 0)));
      current = current.filter((r) => Math.max(r.after ?? 0, r.effectiveAt ?? 0, r.availableAfter ?? 0, r.availableAt ?? 0) === after);
    }
    const record = write ? queue[0] : timed ? current.at(-1) : current[0];
    onMatch?.(record);
    if ((!timed && current.length > 1) || write) queue.splice(queue.indexOf(record), 1);
    let responseBody = record.body;
    if (/"\$(clientAlgoId|newClientOrderId|client_id)"/.test(responseBody)) {
      const raw = ArrayBuffer.isView(body) || body instanceof ArrayBuffer ? new TextDecoder().decode(body) : body;
      const sent = typeof raw === 'string' ? parseRequestJson(raw) : raw ?? {};
      responseBody = responseBody.replace(/"\$(clientAlgoId|newClientOrderId|client_id)"/g,
        (_, field) => JSON.stringify(sent[field] ?? new URL(url).searchParams.get(field), (_key, value) => typeof value === 'bigint' ? JSON.rawJSON(String(value)) : value));
    }
    if (responseBody.includes('"$request:')) {
      const raw = ArrayBuffer.isView(body) || body instanceof ArrayBuffer ? new TextDecoder().decode(body) : body;
      const sent = typeof raw === 'string' ? parseRequestJson(raw) : raw;
      responseBody = responseBody.replace(/"\$request:([\w.]+)"/g, (_, path) =>
        JSON.stringify(path.split('.').reduce((value, key) => value?.[key], sent) ?? null,
          (_key, value) => typeof value === 'bigint' ? JSON.rawJSON(String(value)) : value));
    }
    return { status: record.status, headers: record.headers ?? '', body: responseBody,
      ...(Object.hasOwn(record, 'comparisonRequestBody') ? { comparisonRequestBody: record.comparisonRequestBody } : {}),
      ...(Number.isFinite(record.t) ? { recordedAt: record.t } : {}) };
  }
  unusedWrites() {
    return [...this.queues.values()].flat().filter((r) => (this.isWrite?.(r.method, r.url) ?? r.method !== 'GET')).length;
  }
}
