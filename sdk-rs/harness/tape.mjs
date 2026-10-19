// Shared deterministic replay transport, independent of either engine.
export const createTape = (records, venue, isPrivate) => {
  /** Per-request values: clocks, signatures and generated client ids. */
  const VOLATILE = new Set(['timestamp', 'recvWindow', 'signature', 'newClientOrderId', 'clientAlgoId']);
  const canonicalUrl = (url) => {
    const parsed = new URL(url);
    const query = [...parsed.searchParams]
      .filter(([name]) => !VOLATILE.has(name))
      .map(([name, value]) => [name, value.replace(/"(newClientOrderId|clientAlgoId)":"[^"]*",?/g, '')])
      .sort(([a], [b]) => a.localeCompare(b));
    return `${parsed.origin}${parsed.pathname}?${new URLSearchParams(query)}`;
  };

  // Caller IDs are contractual; only IDs minted by the implementation are volatile.
  const callerIds = new Set(
    records
      .filter((r) => r.type === 'command')
      .flatMap((r) => (r.orders ?? []).map((o) => o.clientOrderId).filter(Boolean)),
  );
  const canonicalBody = (body, url) => {
    if (body == null || body === '') return '';
    const parsed = typeof body === 'string' ? JSON.parse(body) : body;
    const classicBatch =
      venue.rust.startsWith('bitget') && new URL(url).pathname === '/api/v2/mix/order/batch-place-order';
    const walk = (value) => {
      if (Array.isArray(value)) return value.map(walk);
      if (value && typeof value === 'object')
        return Object.fromEntries(
          Object.keys(value)
            .sort()
            .map((key) => {
              const item = value[key];
              if (
                (key === 'newClientOid' || (key === 'clientOid' && !callerIds.has(item))) &&
                typeof item === 'string' &&
                item.length
              )
                return [key, '@generated'];
              // KNOWN_TS_FIXES.bitget: only Classic batch-place's cross spelling.
              return [key, classicBatch && key === 'marginMode' && item === 'cross' ? 'crossed' : walk(item)];
            }),
        );
      return value;
    };
    return JSON.stringify(walk(parsed));
  };
  const requestKey = (method, url, body) => `${method} ${canonicalUrl(url)} ${canonicalBody(body, url)}`;
  const topicKey = (arg) => `${arg.instType ?? ''}|${arg.channel ?? arg.topic}|${arg.instId ?? arg.symbol ?? ''}`;

  /** Serves recorded HTTP responses and websocket frames in tape order. */
  class Tape {
    constructor() {
      this.responses = new Map();
      for (const record of records.filter((r) => r.type === 'http')) {
        const key = requestKey(record.method, record.url, record.requestBody);
        if (!this.responses.has(key)) this.responses.set(key, []);
        this.responses.get(key).push(record);
      }
      this.frames = records.filter((r) => r.type === 'ws' || r.type === 'command');
      this.open = new Map();
      this.waiters = [];
      this.unserved = 0;
      this.generated = new Set();
    }

    respond(method, url, body) {
      const key = requestKey(method, url, body);
      const queue = this.responses.get(key);
      if (!queue?.length) {
        if (this.unserved < 10) console.error(`untaped request: ${key}`);
        this.unserved++;
        return { status: 599, body: '' };
      }
      const record = method === 'GET' ? (queue.length > 1 ? queue.shift() : queue[0]) : queue.shift();
      if (method !== 'GET' && venue.rust.startsWith('bitget') && /(?:place|modify)-/.test(new URL(url).pathname)) {
        const ids = [];
        const visit = (value) => {
          if (!value || typeof value !== 'object') return;
          for (const [key, item] of Object.entries(value)) {
            if (key === 'newClientOid' || (key === 'clientOid' && !callerIds.has(item))) {
              if (
                typeof item !== 'string' ||
                !/^[.A-Za-z0-9:/_-]{1,32}$/.test(item) ||
                (key === 'newClientOid' && callerIds.has(item))
              )
                throw new Error(`invalid generated ${key}`);
              ids.push(item);
            } else visit(item);
          }
        };
        visit(typeof body === 'string' ? JSON.parse(body) : body);
        const reply = JSON.parse(record.body);
        // A rejected signature may be re-signed once with the same identity.
        if (String(reply.code) !== '40009')
          for (const id of ids) {
            if (this.generated.has(id)) throw new Error(`generated client ID reused: ${id}`);
            this.generated.add(id);
          }
      }
      return { status: record.status, body: record.body };
    }

    get unusedWrites() {
      return [...this.responses.values()].reduce((n, queue) => n + queue.filter((r) => r.method !== 'GET').length, 0);
    }

    connect(url, onOpen, onMessage) {
      const socket = { url, onMessage, closed: false, topics: new Set() };
      setImmediate(() => {
        if (socket.closed) return;
        if (!this.open.has(url)) this.open.set(url, new Set());
        this.open.get(url).add(socket);
        onOpen();
        this.waiters = this.waiters.filter((waiter) => !waiter());
      });
      return socket;
    }

    sockets(url) {
      return [...(this.open.get(url) ?? [])].filter((s) => !s.closed);
    }

    sent(socket, text) {
      let message;
      try {
        message = JSON.parse(text);
      } catch {
        return;
      }
      if (!['subscribe', 'unsubscribe'].includes(message.op)) return;
      for (const arg of message.args ?? []) {
        const key = topicKey(arg);
        if (message.op === 'subscribe') socket.topics.add(key);
        else socket.topics.delete(key);
      }
    }

    async socketFor(url) {
      while (!this.sockets(url).length) {
        await new Promise((resolve) => this.waiters.push(() => (this.sockets(url).length ? (resolve(), true) : false)));
      }
      return this.sockets(url)[0];
    }

    async deliver(frame) {
      await this.socketFor(frame.url);
      const sockets = this.sockets(frame.url);
      let arg;
      try {
        arg = JSON.parse(frame.text)?.arg;
      } catch {
        /* literal heartbeat */
      }
      const key = arg && topicKey(arg);
      // Bitget has three sockets at the same URL; recorded frames belong to
      // their subscribed topics, not whichever socket connected last.
      const matching = key ? sockets.filter((socket) => socket.topics.has(key)) : sockets;
      if (key && !matching.length) throw new Error(`frame has no subscribed socket: ${frame.url} ${key}`);
      for (const socket of matching) socket.onMessage(frame.text);
    }

    /**
     * Deliver every frame in order, yielding to the event loop between frames.
     * Commands are awaited where they fall. A private tape is paced: each record
     * waits until its time after the account is ready.
     */
    async play(onTick, ready = async () => {}, runCommand = async () => {}) {
      const urls = [...new Set(this.frames.filter((f) => f.url).map((f) => f.url))];
      for (const url of urls) await this.socketFor(url);
      await ready();
      const start = performance.now();
      let delivered = 0;
      const warm = Math.floor(this.frames.length / 4);
      for (const frame of this.frames) {
        if (delivered === warm) this.warmMark = { thread: process.threadCpuUsage(), wall: performance.now() };
        if (isPrivate) {
          const wait = frame.t - (performance.now() - start);
          if (wait > 0) await new Promise((resolve) => setTimeout(resolve, wait));
        }
        if (frame.type === 'command') {
          this.results.push(
            await runCommand(frame).then(
              (ids) => ({ ok: ids }),
              (error) => ({ error: String(error?.message ?? error) }),
            ),
          );
        } else {
          await this.deliver(frame);
        }
        delivered++;
        onTick?.(frame.t);
        await new Promise(setImmediate);
      }
      return delivered;
    }

    results = [];
  }

  return new Tape();
};
