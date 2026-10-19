import { createHmac, timingSafeEqual } from 'node:crypto';
export class ReplayTimeout extends Error {}
export const within = async (promise, ms, label) => {
  let timer;
  try {
    return await Promise.race([
      promise,
      new Promise((_, reject) => {
        timer = setTimeout(() => reject(new ReplayTimeout(`${label} exceeded ${ms} ms`)), ms);
      }),
    ]);
  } finally {
    clearTimeout(timer);
  }
};

export const createTape = (records, venue, isPrivate) => {
  const KEY = 'replay-key',
    SECRET = 'replay-secret',
    PASSPHRASE = 'replay-passphrase';
  const callerIds = new Set(
    records
      .filter((r) => r.type === 'command')
      .flatMap((r) => (r.orders ?? []).map((o) => o.clientOrderId).filter(Boolean)),
  );
  /** Per-request values: clocks, signatures, request ids and generated client ids. */
  const VOLATILE = new Set(['timestamp', 'recvWindow', 'signature', 'newClientOrderId', 'clientAlgoId', 'requestId']);
  const urlKeys = new Map();
  const bitget = venue.rust.startsWith('bitget');
  const canonicalUrl = (url) => {
    if (urlKeys.has(url)) return urlKeys.get(url);
    const parsed = new URL(url);
    const query = [...parsed.searchParams]
      .filter(([name]) => !VOLATILE.has(name))
      .map(([name, value]) => [name, value.replace(/"(newClientOrderId|clientAlgoId)":"[^"]*",?/g, '')])
      .sort(([a], [b]) => a.localeCompare(b));
    const key = `${parsed.origin}${parsed.pathname}?${new URLSearchParams(query)}`;
    urlKeys.set(url, key);
    return key;
  };

  const canonicalBody = (body, url) => {
    const classicBatch = bitget && new URL(url).pathname === '/api/v2/mix/order/batch-place-order';
    if (body === undefined || body === null || body === '') return '';
    let parsed = body;
    try {
      if (typeof body === 'string') parsed = JSON.parse(body);
      else if (ArrayBuffer.isView(body) || body instanceof ArrayBuffer)
        parsed = JSON.parse(new TextDecoder().decode(body));
    } catch {
      return String(body);
    }
    const sorted = (value) => {
      if (Array.isArray(value)) return value.map(sorted);
      if (value === null || typeof value !== 'object') return value;
      const keys = Object.keys(value)
        .filter((key) => value[key] !== undefined && !(venue.volatileBodyFields && VOLATILE.has(key)))
        .sort();
      return Object.fromEntries(
        keys.map((key) => [
          key,
          ((bitget &&
            (key === 'newClientOid' || key === 'clientOid') &&
            typeof value[key] === 'string' &&
            value[key].length) ||
            venue.generatedBodyId?.(key, value[key])) &&
          !callerIds.has(value[key])
            ? '<generated>'
            : classicBatch && key === 'marginMode' && value[key] === 'cross'
              ? 'crossed'
              : sorted(value[key]),
        ]),
      );
    };
    return JSON.stringify(sorted(parsed));
  };
  const topicKey = (arg) => `${arg.instType ?? ''}|${arg.channel ?? arg.topic}|${arg.instId ?? arg.symbol ?? ''}`;
  const requestKey = (method, url, body) =>
    `${method} ${canonicalUrl(url)}${(bitget || venue.matchRequestBody) && method !== 'GET' ? ` ${canonicalBody(body, url)}` : ''}`;

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
      this.commandIndex = 0;
      this.generated = new Set();
    }

    respond(method, url, body) {
      const key = requestKey(method, url, body);
      const queue = this.responses.get(key);
      if (!queue?.length) {
        this.unserved++;
        if (venue.matchRequestBody) console.error(`Unserved request: ${key}`);
        return { status: 599, body: '' };
      }
      // Staged snapshots become available only once their command starts.
      const eligible = queue.filter((r) => (r.afterCommand ?? 0) <= this.commandIndex);
      if (!eligible.length) {
        this.unserved++;
        console.error(`Request preceded its taped stage: ${key}`);
        return { status: 599, body: '' };
      }
      const stage = Math.max(...eligible.map((r) => r.afterCommand ?? 0));
      const current = eligible.filter((r) => (r.afterCommand ?? 0) === stage);
      const record = current[0];
      // Reads may reconcile repeatedly; every opted-in write needs its own row.
      if (current.length > 1 || method !== 'GET') queue.splice(queue.indexOf(record), 1);
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
      return { status: record.status, headers: record.headers ?? '', body: record.body };
    }

    connect(url, onOpen, onMessage) {
      const socket = { url, onMessage, closed: false, loginVerified: false, topics: new Map() };
      setImmediate(() => {
        if (socket.closed) return;
        // Bybit adds a generated requestId to each connection URL.
        if (!this.open.has(canonicalUrl(url))) this.open.set(canonicalUrl(url), new Set());
        this.open.get(canonicalUrl(url)).add(socket);
        onOpen();
        this.waiters = this.waiters.filter((waiter) => !waiter());
      });
      return socket;
    }

    send(socket, text) {
      if (venue.respondToPing && text === 'ping') {
        setImmediate(() => {
          if (!socket.closed) socket.onMessage('pong');
        });
        return;
      }
      // OKX's opt-in auth validation also uses its v5 subscription argument model.
      const validateTopics = bitget || (venue.validateSubscriptions ?? venue.validateLogin);
      if (!venue.validateLogin && !validateTopics) return;
      if (text === 'ping') return;
      const message = JSON.parse(text);
      if (validateTopics && ['subscribe', 'unsubscribe'].includes(message.op)) {
        if (!Array.isArray(message.args)) throw new Error(`Invalid subscription on ${socket.url}`);
        for (const topic of message.args) {
          if (!topic || typeof (topic.channel ?? topic.topic) !== 'string' || !(topic.channel ?? topic.topic))
            throw new Error(`Invalid subscription topic on ${socket.url}`);
          const key = JSON.stringify(Object.fromEntries(Object.entries(topic).sort(([a], [b]) => a.localeCompare(b))));
          if (message.op === 'subscribe') socket.topics.set(key, topic);
          else socket.topics.delete(key);
        }
        return;
      }
      if (!venue.validateLogin || !/\/(private|business)$/.test(new URL(socket.url).pathname)) return;
      if (message.op !== 'login') return;
      const arg = message.args?.length === 1 ? message.args[0] : undefined;
      const timestamp = String(arg?.timestamp ?? '');
      if (
        arg?.apiKey !== KEY ||
        arg?.passphrase !== PASSPHRASE ||
        !/^\d+$/.test(timestamp) ||
        !Number.isSafeInteger(Number(timestamp)) ||
        Math.abs(Number(timestamp) - Date.now() / 1000) > 30
      ) {
        throw new Error(`Invalid OKX login credentials/timestamp on ${socket.url}`);
      }
      const expected = createHmac('sha256', SECRET).update(`${timestamp}GET/users/self/verify`).digest('base64');
      const supplied = Buffer.from(String(arg.sign ?? ''));
      const expectedBytes = Buffer.from(expected);
      if (supplied.length !== expectedBytes.length || !timingSafeEqual(supplied, expectedBytes)) {
        throw new Error(`Invalid OKX login signature on ${socket.url}`);
      }
      socket.loginVerified = true;
    }

    assertSubscribed(socket, message) {
      if (
        !(venue.validateSubscriptions ?? venue.validateLogin) ||
        !message.arg?.channel ||
        !Array.isArray(message.data)
      )
        return;
      const topics = [...socket.topics.values()].filter((topic) => topic.channel === message.arg.channel);
      // Some documented messages put instId only on each data row, not arg.
      const rows = message.data.length ? message.data : [null];
      for (const row of rows) {
        const object = row && !Array.isArray(row) && typeof row === 'object' ? row : {};
        const covered = topics.some((topic) =>
          Object.entries(topic).every(([key, value]) => {
            if (key === 'channel') return true;
            if (key === 'instType' && value === 'ANY') return true;
            const values = [message.arg[key], object[key]].filter((v) => v !== undefined && v !== '');
            if (!values.length && key === 'instType') {
              const id = object.instId ?? message.arg.instId;
              if (id?.endsWith('-SWAP')) values.push('SWAP');
            }
            return values.length > 0 && values.every((v) => v === value);
          }),
        );
        if (!covered) {
          throw new Error(
            `Unsubscribed ${message.arg.channel} frame on ${socket.url}: ${JSON.stringify(message.arg)}${object.instId ? ` instId=${object.instId}` : ''}`,
          );
        }
      }
    }

    assertWritesConsumed() {
      const missing = [...this.responses].flatMap(([key, rows]) => (key.startsWith('GET ') ? [] : rows.map(() => key)));
      if (missing.length) throw new Error(`${missing.length} taped write(s) were not sent:\n${missing.join('\n')}`);
    }

    sockets(url) {
      return [...(this.open.get(canonicalUrl(url)) ?? [])].filter((s) => !s.closed);
    }

    sent(socket, text) {
      this.send(socket, text);
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
      const key = bitget && arg && topicKey(arg);
      // Bitget has three sockets at the same URL; recorded frames belong to
      // their subscribed topics, not whichever socket connected last.
      const matching = key
        ? sockets.filter((socket) => [...socket.topics.values()].some((topic) => topicKey(topic) === key))
        : sockets;
      if (key && !matching.length) throw new Error(`frame has no subscribed socket: ${frame.url} ${key}`);
      for (const socket of matching) {
        if ((venue.validateLogin || venue.validateSubscriptions) && frame.text.trimStart().startsWith('{')) {
          const message = JSON.parse(frame.text);
          if (venue.validateLogin && message.event === 'login' && String(message.code) === '0' && !socket.loginVerified)
            throw new Error(`Taped login success preceded a valid login on ${frame.url}`);
          this.assertSubscribed(socket, message);
        }
        socket.onMessage(frame.text);
      }
    }

    /**
     * Deliver every frame in order, yielding to the event loop between frames.
     * Commands are awaited where they fall. A private tape is paced: each record
     * waits until its time after the account is ready.
     */
    async play(onTick, ready = async () => {}, runCommand = async () => {}) {
      const urls = [...new Set(this.frames.filter((f) => f.url).map((f) => f.url))];
      await within(
        (async () => {
          for (const url of urls) await this.socketFor(url);
          await ready();
        })(),
        15_000,
        'Replay startup',
      );
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
          this.commandIndex++;
          this.results.push(
            await within(runCommand(frame), 15_000, `Command ${frame.op}`).then(
              (ids) => ({ ok: ids }),
              (error) => {
                if (error instanceof ReplayTimeout) throw error;
                return { error: String(error?.message ?? error) };
              },
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

    get unusedWrites() {
      return [...this.responses.values()].reduce((n, queue) => n + queue.filter((r) => r.method !== 'GET').length, 0);
    }

    results = [];
  }

  return new Tape();
};
