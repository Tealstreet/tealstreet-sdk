import { isWriteRequest, selectResponse } from './requests.mjs';
import { parseRequestJson } from './request-json.mjs';

// Reads may reuse their last snapshot; a mutation response is consumed once.
export class Responses {
  constructor(records, key, check, isWrite) {
    this.key = key;
    this.check = check;
    this.isWrite = isWrite ?? isWriteRequest;
    this.unserved = 0;
    this.queues = new Map();
    for (const record of records.filter((r) => r.type === 'http')) {
      const id = key(record.method, record.url, record.requestBody);
      if (!this.queues.has(id)) this.queues.set(id, []);
      this.queues.get(id).push(record);
    }
  }
  respond(method, url, body, commandIndex = 0, elapsed = 0) {
    const queue = this.check?.(method, url, body) === false ? undefined : this.queues.get(this.key(method, url, body));
    if (!queue?.length) {
      this.unserved++;
      return { status: 599, body: '' };
    }
    const record = selectResponse(queue, elapsed, { commandIndex, write: this.isWrite(method, url) });
    if (!record) {
      this.unserved++;
      return { status: 599, body: '' };
    }
    // WEEX acknowledges generated IDs from the actual request, never a fixed ID.
    let responseBody = record.body;
    if (/"\$(clientAlgoId|newClientOrderId)"/.test(responseBody)) {
      const sent = typeof body === 'string' ? parseRequestJson(body) : body;
      responseBody = responseBody.replace(/"\$(clientAlgoId|newClientOrderId)"/g,
        (_, field) => JSON.stringify(sent?.[field] ?? null,
          (_key, value) => typeof value === 'bigint' ? JSON.rawJSON(String(value)) : value));
    }
    return { status: record.status, headers: record.headers ?? '', body: responseBody };
  }
  unusedWrites() {
    return [...this.queues.values()].flat().filter((r) => this.isWrite(r.method, r.url)).length;
  }
}
