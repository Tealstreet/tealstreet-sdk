/** Per-request values: clocks, signatures and generated client ids. */
const VOLATILE = new Set(['timestamp', 'recvWindow', 'signature', 'newClientOrderId', 'clientAlgoId']);
export const canonicalUrl = (url) => {
  const parsed = new URL(url);
  const query = [...parsed.searchParams]
    .filter(([name]) => !VOLATILE.has(name))
    .map(([name, value]) => [name, value.replace(/"(newClientOrderId|clientAlgoId)":"[^"]*",?/g, '')])
    .sort(([a], [b]) => a.localeCompare(b));
  return `${parsed.origin}${parsed.pathname}?${new URLSearchParams(query)}`;
};

export const canonicalBody = (body, callerIds = new Set()) => {
  if (!body) return '';
  const clean = (value, key = '') => {
    if (Array.isArray(value)) return value.map((v) => clean(v));
    if (value && typeof value === 'object')
      return Object.fromEntries(
        Object.entries(value)
          .filter(([k, v]) => !(VOLATILE.has(k) || (k === 'clientId' && !callerIds.has(String(v)))))
          .sort(([a], [b]) => a.localeCompare(b))
          .map(([k, v]) => [k, clean(v, k)]),
      );
    if (typeof value === 'string' && !/id$/i.test(key) && /^-?(?:0|[1-9]\d*)(?:\.\d+)?$/.test(value)) {
      const [whole, fraction] = value.split('.');
      const trimmed = fraction?.replace(/0+$/, '');
      return trimmed ? `${whole}.${trimmed}` : whole;
    }
    return value;
  };
  try {
    return JSON.stringify(clean(typeof body === 'string' ? JSON.parse(body) : body));
  } catch {
    return String(body);
  }
};

export const withTimeout = async (promise, label, ms = 30000) => {
  let timer;
  try {
    return await Promise.race([
      promise,
      new Promise((_, reject) => {
        timer = setTimeout(() => reject(new Error(`replay timeout: ${label}`)), ms);
      }),
    ]);
  } finally {
    clearTimeout(timer);
  }
};

/** Serves recorded HTTP responses and websocket frames in tape order. */
export class Tape {
  constructor(records, venue, isPrivate, timeoutMs = 30000) {
    this.venue = venue;
    this.isPrivate = isPrivate;
    this.timeoutMs = timeoutMs;
    this.callerIds = new Set(
      records
        .flatMap((r) => r.orders ?? [])
        .map((o) => o.clientOrderId)
        .filter((id) => id !== undefined),
    );
    this.responses = new Map();
    this.writeOrder = [];
    for (const record of records.filter((r) => r.type === 'http')) {
      const key = `${record.method} ${canonicalUrl(record.url)}${record.requestBody === undefined ? '' : ` BODY ${canonicalBody(record.requestBody, this.callerIds)}`}`;
      if (!this.responses.has(key)) this.responses.set(key, []);
      this.responses.get(key).push(record);
      if (record.method !== 'GET') this.writeOrder.push(record);
    }
    this.frames = records.filter((r) => r.type === 'ws' || r.type === 'command');
    this.open = new Map();
    this.waiters = new Set();
    this.unserved = 0;
  }

  invalidSignatures = 0;
  subscriptions = new Set();
  get missingWrites() {
    return [...this.responses].filter(([key]) => !key.startsWith('GET ')).reduce((n, [, queue]) => n + queue.length, 0);
  }
  respond(method, url, body, headers) {
    if (this.isPrivate && this.venue.verifyRequest && !this.venue.verifyRequest({ method, url, body, headers })) {
      this.invalidSignatures++;
      throw new Error(`invalid replay signature: ${method} ${url}`);
    }
    const key = `${method} ${canonicalUrl(url)}`;
    const queue = this.responses.get(`${key} BODY ${canonicalBody(body, this.callerIds)}`) ?? this.responses.get(key);
    if (!queue?.length) {
      if (this.unserved < 10) console.error(`unserved ${method} ${url} ${canonicalBody(body, this.callerIds)}`);
      this.unserved++;
      return { status: 599, body: '' };
    }
    if (method !== 'GET' && this.venue.compareWriteOrder && this.writeOrder[0] !== queue[0]) {
      this.unserved++;
      return { status: 599, body: '' };
    }
    if (method !== 'GET') this.writeOrder.shift();
    const record = method !== 'GET' || queue.length > 1 ? queue.shift() : queue[0];
    return { status: record.status, body: record.body };
  }

  connect(url, onOpen, onMessage) {
    const socket = { url, onMessage, closed: false, subscriptions: new Set() };
    setImmediate(() => {
      if (socket.closed) return;
      if (!this.open.has(url)) this.open.set(url, []);
      this.open.get(url).push(socket);
      onOpen();
      for (const waiter of [...this.waiters]) waiter();
    });
    return socket;
  }

  send(socket, text) {
    try {
      const message = JSON.parse(text);
      if (this.isPrivate && this.venue.verifySubscribe && !this.venue.verifySubscribe(message)) {
        this.invalidSignatures++;
        throw new Error('invalid replay websocket signature');
      }
      for (const topic of message.params ?? []) {
        if (message.method === 'SUBSCRIBE') {
          socket.subscriptions.add(topic);
          if (
            !this.venue.marketFilter ||
            topic.startsWith('account.') ||
            this.venue.marketFilter({ symbol: topic.split('.').at(-1) })
          )
            this.subscriptions.add(`${message.method} ${topic}`);
        }
        if (message.method === 'UNSUBSCRIBE') socket.subscriptions.delete(topic);
      }
      for (const waiter of [...this.waiters]) waiter();
    } catch (error) {
      if (!(error instanceof SyntaxError)) throw error;
    }
  }

  async socketFor(url, text) {
    const stream = text && this.venue.streamOf?.(text);
    const find = () => {
      const sockets = (this.open.get(url) ?? []).filter((s) => !s.closed);
      return stream
        ? sockets.find((s) => [...s.subscriptions].some((topic) => stream === topic || stream.startsWith(`${topic}.`)))
        : sockets[0];
    };
    const ready = find();
    if (ready) return ready;
    let waiter;
    try {
      return await withTimeout(
        new Promise((resolve) => {
          waiter = () => {
            const socket = find();
            if (socket) resolve(socket);
          };
          this.waiters.add(waiter);
          waiter();
        }),
        `socket ${url} ${stream ?? 'open'}`,
        this.timeoutMs,
      );
    } finally {
      this.waiters.delete(waiter);
    }
  }

  /**
   * Deliver every frame in order, yielding to the event loop between frames.
   * Commands are awaited where they fall. A private tape is paced: each record
   * waits until its time after the account is ready.
   */
  async play(onTick, ready = async () => {}, runCommand = async () => {}) {
    const urls = [...new Set(this.frames.filter((f) => f.url).map((f) => f.url))];
    for (const url of urls) await this.socketFor(url);
    await withTimeout(ready(), 'account/books readiness', this.timeoutMs);
    const start = performance.now();
    let delivered = 0;
    const warm = Math.floor(this.frames.length / 4);
    for (const frame of this.frames) {
      if (delivered === warm) this.warmMark = { thread: process.threadCpuUsage(), wall: performance.now() };
      if (this.isPrivate) {
        const wait = frame.t - (performance.now() - start);
        if (wait > 0) await new Promise((resolve) => setTimeout(resolve, wait));
      }
      if (frame.type === 'command') {
        this.results.push(
          await withTimeout(runCommand(frame), `command ${frame.op}`, this.timeoutMs).then(
            (ids) => ({ ok: ids }),
            (error) => {
              if (String(error?.message).startsWith('replay timeout:')) throw error;
              return { error: String(error?.message ?? error) };
            },
          ),
        );
      } else {
        const socket = await this.socketFor(frame.url, frame.text);
        socket.onMessage(frame.text);
      }
      delivered++;
      onTick?.(frame.t);
      await new Promise(setImmediate);
    }
    return delivered;
  }

  results = [];
}
