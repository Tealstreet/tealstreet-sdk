// Runs a WasmEngine against a transport: executes its actions (sockets, HTTP,
// timers) and feeds the results back in. The same host serves browser workers,
// Cloudflare Workers, and the replay harness; only the transport differs.
//
// transport = {
//   now(): number,
//   connect(url, { onOpen, onMessage, onClose }): { send(text), close() },
//   request({ method, url, headers, body }): Promise<{ status, body }>,
//   setTimeout(fn, ms), clearTimeout(handle),
// }

export class EngineHost {
  constructor(engine, transport, handlers = {}) {
    this.engine = engine;
    this.transport = transport;
    this.handlers = handlers;
    this.sockets = new Map();
    this.timers = new Map();
    this.commands = new Map();
    this.nextCommand = 1;
  }

  start() {
    this.run(this.engine.start(this.transport.now()));
  }

  /** Send a command; resolves with its result for commands that report one. */
  command(command) {
    const reply = command.op === 'subscribeOrderBook' || command.op === 'unsubscribeOrderBook'
      || command.op === 'subscribeCandles' || command.op === 'unsubscribeCandles'
      ? undefined
      : this.nextCommand++;
    const promise = reply === undefined
      ? Promise.resolve()
      : new Promise((resolve, reject) => this.commands.set(reply, { resolve, reject }));
    this.run(this.engine.command(JSON.stringify(reply === undefined ? command : { ...command, command: reply }), this.transport.now()));
    return promise;
  }

  /** Drain Store and book changes. Call once per flush interval. */
  flush() {
    const update = this.engine.storeUpdate();
    if (update) this.handlers.onStoreUpdate?.(JSON.parse(update));
    const changed = this.engine.changedBooks();
    if (changed && this.handlers.onBook) {
      for (const symbol of JSON.parse(changed)) {
        const book = this.engine.book(symbol);
        if (book) this.handlers.onBook(symbol, JSON.parse(book));
      }
    }
  }

  dispose() {
    for (const socket of this.sockets.values()) socket.close();
    for (const handle of this.timers.values()) this.transport.clearTimeout(handle);
    this.sockets.clear();
    this.timers.clear();
  }

  run(actionsJson) {
    if (!actionsJson) return;
    for (const action of JSON.parse(actionsJson)) this.execute(action);
  }

  execute(action) {
    const { engine, transport } = this;
    switch (action.op) {
      case 'wsConnect': {
        const id = action.socket;
        const socket = transport.connect(action.url, {
          onOpen: () => this.run(engine.wsOpen(id, transport.now())),
          onMessage: (text) => this.run(engine.wsMessage(id, text, transport.now())),
          onClose: (code) => {
            this.sockets.delete(id);
            this.run(engine.wsClose(id, code ?? 1006, transport.now()));
          },
        });
        this.sockets.set(id, socket);
        break;
      }
      case 'wsSend':
        this.sockets.get(action.socket)?.send(action.text);
        break;
      case 'wsClose':
        this.sockets.get(action.socket)?.close();
        this.sockets.delete(action.socket);
        break;
      case 'http': {
        const id = action.request;
        transport
          .request({
            method: action.method,
            url: action.url,
            headers: Object.fromEntries(action.headers),
            body: action.body,
          })
          .then(
            ({ status, body }) => this.run(engine.httpResponse(id, status, body, transport.now())),
            (error) => this.run(engine.httpFailed(id, String(error?.message ?? error), transport.now())),
          );
        break;
      }
      case 'setTimer': {
        const id = action.timer;
        const handle = transport.setTimeout(() => {
          this.timers.delete(id);
          this.run(engine.timer(id, transport.now()));
        }, Math.max(0, action.at - transport.now()));
        this.timers.set(id, handle);
        break;
      }
      case 'clearTimer':
        transport.clearTimeout(this.timers.get(action.timer));
        this.timers.delete(action.timer);
        break;
      case 'log':
        this.handlers.onLog?.(action.level, action.message);
        break;
      case 'done': {
        const pending = this.commands.get(action.command);
        this.commands.delete(action.command);
        if (!pending) break;
        if (action.error !== undefined) pending.reject(new Error(action.error));
        else pending.resolve(action.result === undefined ? undefined : JSON.parse(action.result));
        break;
      }
      case 'fill':
        this.handlers.onFill?.(action);
        break;
      case 'candle':
        this.handlers.onCandle?.(action.symbol, action.interval, action.candle);
        break;
      default:
        throw new Error(`unknown engine action: ${action.op}`);
    }
  }
}

/** Transport over the runtime's own WebSocket, fetch and timers. */
export const nativeTransport = () => ({
  now: () => Date.now(),
  connect(url, { onOpen, onMessage, onClose }) {
    const ws = new WebSocket(url);
    ws.addEventListener('open', () => onOpen());
    ws.addEventListener('message', (event) => {
      if (typeof event.data === 'string') onMessage(event.data);
    });
    ws.addEventListener('close', (event) => onClose(event.code));
    return { send: (text) => ws.send(text), close: () => ws.close() };
  },
  async request({ method, url, headers, body }) {
    const response = await fetch(url, { method, headers, body });
    return { status: response.status, body: await response.text() };
  },
  setTimeout: (fn, ms) => setTimeout(fn, ms),
  clearTimeout: (handle) => clearTimeout(handle),
});
