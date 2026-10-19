import { isRead, takeResponse } from './requests.mjs';

// Reads may reuse their last snapshot; a mutation response is consumed once.
export class Responses {
  constructor(records, key, check, isWrite = (method, url) => !isRead(method, url), checkUnusedWrites = true) {
    this.key = key;
    this.check = check;
    this.isWrite = isWrite;
    this.checkUnusedWrites = checkUnusedWrites;
    this.unserved = 0;
    this.queues = new Map();
    for (const record of records.filter((r) => r.type === 'http')) {
      const id = key(record.method, record.url, record.requestBody);
      if (!this.queues.has(id)) this.queues.set(id, []);
      this.queues.get(id).push(record);
    }
  }
  respond(method, url, body, commandIndex = 0, time = 0, headers) {
    const queue = this.check?.(method, url, body, headers) === false ? undefined : this.queues.get(this.key(method, url, body));
    const record = takeResponse(queue, method, url, time, commandIndex, !this.isWrite(method, url));
    if (!record) {
      this.unserved++;
      return { status: 599, body: '' };
    }
    return { status: record.status, headers: record.headers ?? '', body: record.body };
  }
  unusedWrites() {
    if (!this.checkUnusedWrites) return 0;
    return [...this.queues.values()].flat().filter((r) => this.isWrite?.(r.method, r.url)).length;
  }
}
