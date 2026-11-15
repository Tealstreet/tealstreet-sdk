/*!
 * Reconnecting WebSocket
 * by Pedro Ladaria <pedro.ladaria@gmail.com>
 * https://github.com/pladaria/reconnecting-websocket
 * License MIT
 */

import * as Events from './events';

const getGlobalWebSocket = (): WebSocket | undefined => {
  if (typeof WebSocket !== 'undefined') {
    // @ts-ignore
    return WebSocket;
  }
};

/**
 * Returns true if given argument looks like a WebSocket class
 */
const isWebSocket = (w: any) => typeof w !== 'undefined' && !!w && w.CLOSING === 2;

const diagnosticUrl = (value: string): string => {
  try {
    const url = new URL(value);
    return url.protocol === 'ws:' || url.protocol === 'wss:' ? `${url.origin}${url.pathname}` : '[unavailable]';
  } catch {
    return '[unavailable]';
  }
};

export type Event = Events.Event;
export type ErrorEvent = Events.ErrorEvent;
export type CloseEvent = Events.CloseEvent;

export type HeadersProvider = () => Record<string, string>;

export type Options = {
  WebSocket?: any;
  maxReconnectionDelay?: number;
  minReconnectionDelay?: number;
  reconnectionDelayGrowFactor?: number;
  minUptime?: number;
  connectionTimeout?: number;
  maxRetries?: number;
  maxEnqueuedMessages?: number;
  startClosed?: boolean;
  debug?: boolean;
  /**
   * Headers to send with each WebSocket connection (Node.js ws library only).
   * A provider is evaluated immediately before every Node/Bun reconnect so
   * short-lived authentication headers can be regenerated safely.
   */
  headers?: Record<string, string> | HeadersProvider;
  /** A close this answers true for reconnects with no backoff wait; later failures back off as usual. */
  reconnectsAtOnce?: (event: { code: number; reason: string }) => boolean;
  /** Called on every change of `loopState.phase`, so a supervisor can check each transition. */
  onLoopPhase?: (phase: LoopPhase) => void;
};

/**
 * Where the connect loop is. While it should reconnect it is always in one of the
 * first three; `stopped` means nothing will open a socket again until `reconnect()`.
 */
export type LoopPhase = 'connected' | 'connecting' | 'retry-scheduled' | 'stopped';

const DEFAULT = {
  maxReconnectionDelay: 10000,
  minReconnectionDelay: 1000 + Math.random() * 4000,
  minUptime: 5000,
  reconnectionDelayGrowFactor: 1.3,
  connectionTimeout: 4000,
  maxRetries: Infinity,
  maxEnqueuedMessages: Infinity,
  startClosed: false,
  debug: false,
};

export type UrlProvider = string | (() => string) | (() => Promise<string>);

export type Message = string | Blob | BufferSource;

export type ListenersMap = {
  error: Array<Events.WebSocketEventListenerMap['error']>;
  message: Array<Events.WebSocketEventListenerMap['message']>;
  open: Array<Events.WebSocketEventListenerMap['open']>;
  close: Array<Events.WebSocketEventListenerMap['close']>;
};

export class ReconnectingWebSocket {
  private _ws?: WebSocket;
  private _listeners: ListenersMap = {
    error: [],
    message: [],
    open: [],
    close: [],
  };
  private _retryCount = -1;
  private _uptimeTimeout: any;
  private _connectTimeout: any;
  private _shouldReconnect = true;
  private _connectLock = false;
  /** Bumped by every connect; a URL provider settling for an older one is ignored. */
  private _attempt = 0;
  private _binaryType: BinaryType = 'blob';
  private _closeCalled = false;
  private _messageQueue: Message[] = [];
  private _loopState: { phase: LoopPhase; since: number } = { phase: 'stopped', since: Date.now() };

  private readonly _url: UrlProvider;
  private readonly _protocols?: string | string[];
  private readonly _options: Options;

  constructor(url: UrlProvider, protocols?: string | string[], options: Options = {}) {
    this._url = url;
    this._protocols = protocols;
    this._options = options;
    if (this._options.startClosed) {
      this._shouldReconnect = false;
    }
    this._connect();
  }

  static get CONNECTING() {
    return 0;
  }
  static get OPEN() {
    return 1;
  }
  static get CLOSING() {
    return 2;
  }
  static get CLOSED() {
    return 3;
  }

  get CONNECTING() {
    return ReconnectingWebSocket.CONNECTING;
  }
  get OPEN() {
    return ReconnectingWebSocket.OPEN;
  }
  get CLOSING() {
    return ReconnectingWebSocket.CLOSING;
  }
  get CLOSED() {
    return ReconnectingWebSocket.CLOSED;
  }

  get binaryType() {
    return this._ws ? this._ws.binaryType : this._binaryType;
  }

  set binaryType(value: BinaryType) {
    this._binaryType = value;
    if (this._ws) {
      this._ws.binaryType = value;
    }
  }

  /**
   * Returns the number or connection retries
   */
  /** The connect loop's phase and when it entered it. */
  get loopState(): { phase: LoopPhase; since: number } {
    return { ...this._loopState };
  }

  get retryCount(): number {
    return Math.max(this._retryCount, 0);
  }

  /**
   * The number of bytes of data that have been queued using calls to send() but not yet
   * transmitted to the network. This value resets to zero once all queued data has been sent.
   * This value does not reset to zero when the connection is closed; if you keep calling send(),
   * this will continue to climb. Read only
   */
  get bufferedAmount(): number {
    const bytes = this._messageQueue.reduce((acc, message) => {
      if (typeof message === 'string') {
        acc += message.length; // not byte size
      } else if (message instanceof Blob) {
        acc += message.size;
      } else {
        acc += message.byteLength;
      }
      return acc;
    }, 0);
    return bytes + (this._ws ? this._ws.bufferedAmount : 0);
  }

  /**
   * The extensions selected by the server. This is currently only the empty string or a list of
   * extensions as negotiated by the connection
   */
  get extensions(): string {
    return this._ws ? this._ws.extensions : '';
  }

  /**
   * A string indicating the name of the sub-protocol the server selected;
   * this will be one of the strings specified in the protocols parameter when creating the
   * WebSocket object
   */
  get protocol(): string {
    return this._ws ? this._ws.protocol : '';
  }

  /**
   * The current state of the connection; this is one of the Ready state constants
   */
  get readyState(): number {
    if (this._ws) {
      return this._ws.readyState;
    }
    return this._options.startClosed ? ReconnectingWebSocket.CLOSED : ReconnectingWebSocket.CONNECTING;
  }

  /**
   * The URL as resolved by the constructor
   */
  get url(): string {
    return this._ws ? this._ws.url : '';
  }

  /**
   * An event listener to be called when the WebSocket connection's readyState changes to CLOSED
   */
  public onclose: ((event: Events.CloseEvent) => void) | null = null;

  /**
   * An event listener to be called when an error occurs
   */
  public onerror: ((event: Events.ErrorEvent) => void) | null = null;

  /**
   * An event listener to be called when a message is received from the server
   */
  public onmessage: ((event: MessageEvent) => void) | null = null;

  /**
   * An event listener to be called when the WebSocket connection's readyState changes to OPEN;
   * this indicates that the connection is ready to send and receive data
   */
  public onopen: ((event: Event) => void) | null = null;

  /**
   * Closes the WebSocket connection or connection attempt, if any. If the connection is already
   * CLOSED, this method does nothing
   */
  public close(code = 1000, reason?: string) {
    this._closeCalled = true;
    this._shouldReconnect = false;
    this._clearTimeouts();
    this._enterPhase('stopped');
    if (!this._ws) {
      this._debug('close enqueued: no ws instance');
      return;
    }
    if (this._ws.readyState === this.CLOSED) {
      this._debug('close: already closed');
      return;
    }
    this._ws.close(code, reason);
  }

  public terminate(code = 1000, reason?: string) {
    this._closeCalled = true;
    this._shouldReconnect = false;
    this._clearTimeouts();
    this._enterPhase('stopped');
    if (!this._ws) return;
    this._removeListeners();
    const ws = this._ws as any;
    if (typeof ws.terminate === 'function') {
      ws.terminate();
    } else if (this._ws.readyState !== this.CLOSED) {
      this._ws.close(code, reason);
    }
    this._ws = undefined;
  }

  /**
   * Closes the WebSocket connection or connection attempt and connects again.
   * Resets retry counter;
   */
  public reconnect(code?: number, reason?: string) {
    console.info('[ReconnectingWebSocket] reconnect() requested');
    this._shouldReconnect = true;
    this._closeCalled = false;
    this._connectLock = false; // Reset in case previous _connect() left it stuck
    this._retryCount = -1;
    if (!this._ws || this._ws.readyState === this.CLOSED) {
      this._connect();
    } else {
      this._disconnect(code, reason);
      this._connect();
    }
  }

  /** A supervisor timeout keeps the current wrapper's retry count and backoff. */
  public retryConnection(): boolean {
    if (!this._shouldReconnect || this._loopState.phase !== 'connected') return false;
    this._disconnect(1000, 'supervisor timeout');
    return true;
  }

  /** A supervisor handshake timeout preserves the same reconnect policy. */
  public timeoutConnecting(): boolean {
    if (!this._shouldReconnect || this._loopState.phase !== 'connecting' || this._ws?.readyState === this.OPEN)
      return false;
    // Invalidate deferred URL work before a close listener can start another attempt.
    this._attempt++;
    this._connectLock = false;
    this._handleTimeout();
    return true;
  }

  /**
   * Enqueue specified data to be transmitted to the server over the WebSocket connection
   */
  public send(data: Message) {
    if (this._ws && this._ws.readyState === this.OPEN) {
      this._debug('send');
      this._ws.send(data);
    } else {
      const { maxEnqueuedMessages = DEFAULT.maxEnqueuedMessages } = this._options;
      if (this._messageQueue.length < maxEnqueuedMessages) {
        this._debug('enqueue');
        this._messageQueue.push(data);
      }
    }
  }

  /**
   * Register an event handler of a specific event type
   */
  public addEventListener<T extends keyof Events.WebSocketEventListenerMap>(
    type: T,
    listener: Events.WebSocketEventListenerMap[T],
  ): void {
    if (this._listeners[type]) {
      // @ts-ignore
      this._listeners[type].push(listener);
    }
  }

  public dispatchEvent(event: Event) {
    const listeners = this._listeners[event.type as keyof Events.WebSocketEventListenerMap];
    if (listeners) {
      for (const listener of listeners) {
        this._callEventListener(event, listener);
      }
    }
    return true;
  }

  /**
   * Removes an event listener
   */
  public removeEventListener<T extends keyof Events.WebSocketEventListenerMap>(
    type: T,
    listener: Events.WebSocketEventListenerMap[T],
  ): void {
    if (this._listeners[type]) {
      // @ts-ignore
      this._listeners[type] = this._listeners[type].filter((l) => l !== listener);
    }
  }

  private _debug(...args: any[]) {
    if (this._options.debug) {
      // not using spread because compiled version uses Symbols
      // tslint:disable-next-line
      console.log.apply(console, ['RWS>', ...args]);
    }
  }

  private _getNextDelay() {
    const {
      reconnectionDelayGrowFactor = DEFAULT.reconnectionDelayGrowFactor,
      minReconnectionDelay = DEFAULT.minReconnectionDelay,
      maxReconnectionDelay = DEFAULT.maxReconnectionDelay,
    } = this._options;
    let delay = 0;
    if (this._retryCount > 0) {
      delay = minReconnectionDelay * Math.pow(reconnectionDelayGrowFactor, this._retryCount - 1);
      if (delay > maxReconnectionDelay) {
        delay = maxReconnectionDelay;
      }
    }
    this._debug('next delay', delay);
    return delay;
  }

  private _wait(delay: number): Promise<void> {
    return new Promise((resolve) => {
      setTimeout(resolve, delay);
    });
  }

  private _enterPhase(phase: LoopPhase) {
    if (this._loopState.phase === phase) return;
    this._loopState = { phase, since: Date.now() };
    this._options.onLoopPhase?.(phase);
  }

  private _getNextUrl(urlProvider: UrlProvider): Promise<string> {
    if (typeof urlProvider === 'string') {
      return Promise.resolve(urlProvider);
    }
    if (typeof urlProvider === 'function') {
      const url = urlProvider();
      if (typeof url === 'string') {
        return Promise.resolve(url);
      }
      // @ts-ignore redundant check
      if (url.then) {
        return url;
      }
    }
    throw Error('Invalid URL');
  }

  private _connect() {
    if (this._connectLock || !this._shouldReconnect) {
      return;
    }
    this._connectLock = true;

    const {
      maxRetries = DEFAULT.maxRetries,
      connectionTimeout = DEFAULT.connectionTimeout,
      WebSocket = getGlobalWebSocket(),
    } = this._options;

    if (this._retryCount >= maxRetries) {
      this._debug('max retries reached', this._retryCount, '>=', maxRetries);
      this._connectLock = false;
      this._enterPhase('stopped');
      // Dispatch close event so listeners know we gave up
      const event = new Events.CloseEvent(4000, 'Max retries exhausted', this);
      if (this.onclose) {
        this.onclose(event);
      }
      this._listeners.close.forEach((listener) => this._callEventListener(event, listener));
      return;
    }

    this._retryCount++;
    // A reconnect may start while an older attempt still awaits its URL provider:
    // that one must not open a second socket or unlock this one when it settles.
    const attempt = ++this._attempt;

    this._debug('connect', this._retryCount);
    this._removeListeners();
    if (!isWebSocket(WebSocket)) {
      this._connectLock = false;
      throw Error('No valid WebSocket class provided');
    }
    const delay = this._getNextDelay();
    this._enterPhase(delay > 0 ? 'retry-scheduled' : 'connecting');
    this._wait(delay)
      .then(() => {
        if (attempt !== this._attempt || !this._shouldReconnect) return;
        this._enterPhase('connecting');
        return this._getNextUrl(this._url);
      })
      .then((url) => {
        if (attempt !== this._attempt || url === undefined) return;
        // close could be called before creating the ws
        if (this._closeCalled) {
          this._connectLock = false;
          this._enterPhase('stopped');
          return;
        }
        console.info(`[ReconnectingWebSocket] connecting - URL: ${diagnosticUrl(url)}`);

        // In Node.js, the ws library accepts options including headers
        // This is needed for CloudFront/proxies that require Origin header
        // ws signature: new WebSocket(url, protocols, options)
        // IMPORTANT: We must force HTTP/1.1 via ALPNProtocols because WebSocket
        // upgrade mechanism doesn't work over HTTP/2
        const { headers } = this._options;
        const isNode = typeof process !== 'undefined' && process.versions?.node;
        const isBun = typeof process !== 'undefined' && !!(process as any).isBun;
        // BaseWebSocket supplies `WebSocket` only after resolving the
        // header-capable `ws` constructor. Trust that explicit transport even
        // when a renderer-like runtime does not expose a reliable Node marker.
        const usesHeaderCapableTransport = !!this._options.WebSocket && !!headers;
        if (isNode || isBun || usesHeaderCapableTransport) {
          // Node.js/Bun environment - pass headers and force HTTP/1.1 via ALPNProtocols
          const wsOptions: Record<string, unknown> = {
            // Force HTTP/1.1 - WebSocket upgrade doesn't work over HTTP/2
            ALPNProtocols: ['http/1.1'],
          };
          let resolvedHeaders: Record<string, string> | undefined;
          try {
            resolvedHeaders = typeof headers === 'function' ? headers() : headers;
          } catch {
            // Header providers can produce short-lived auth material. Keep an
            // exception observable without exposing provider error text, which
            // may contain credentials, then retry through the normal backoff.
            this._connectLock = false;
            if (!this._closeCalled && this._shouldReconnect) {
              const event = new Events.ErrorEvent(Error('WebSocket header provider failed'), this);
              if (this.onerror) {
                this.onerror(event);
              }
              this._listeners.error.forEach((listener) => this._callEventListener(event, listener));
              this._connect();
            }
            return;
          }
          if (resolvedHeaders) {
            wsOptions.headers = resolvedHeaders;
          }
          this._ws = new WebSocket(url, this._protocols || [], wsOptions);
        } else {
          // Browser environment - standard WebSocket API
          this._ws = this._protocols ? new WebSocket(url, this._protocols) : new WebSocket(url);
        }
        // Only set binaryType in browser - ws package doesn't support 'blob'
        if (!(isNode || isBun)) {
          this._ws!.binaryType = this._binaryType;
        }
        this._connectLock = false;
        this._addListeners();

        this._connectTimeout = setTimeout(() => this._handleTimeout(), connectionTimeout);
      })
      .catch(() => {
        if (attempt !== this._attempt) return;
        this._connectLock = false;
        // A URL provider that failed (no token yet, a refused key) retries through the
        // normal bounded reconnect delay; stopping here left the socket with no loop.
        if (!this._closeCalled && this._shouldReconnect) {
          console.info('[ReconnectingWebSocket] URL: [unavailable]; connection attempt failed');
          const event = new Events.ErrorEvent(Error('WebSocket URL provider failed'), this);
          if (this.onerror) {
            this.onerror(event);
          }
          this._listeners.error.forEach((listener) => this._callEventListener(event, listener));
          this._connect();
        }
      });
  }

  private _handleTimeout() {
    this._debug('timeout event');
    this._handleError(new Events.ErrorEvent(Error('TIMEOUT'), this));
  }

  private _disconnect(code = 1000, reason?: string, initiator: 'local' | 'unknown' = 'local') {
    this._clearTimeouts();
    this._removeListeners();
    try {
      this._ws?.close(code, reason);
    } catch (error) {
      // A native close refusal must not suppress the wrapper's close notification.
    }
    this._handleClose(new Events.CloseEvent(code, reason, this, initiator));
  }

  private _acceptOpen() {
    this._debug('accept open');
    this._retryCount = 0;
  }

  private _callEventListener<T extends keyof Events.WebSocketEventListenerMap>(
    event: Events.WebSocketEventMap[T],
    listener: Events.WebSocketEventListenerMap[T],
  ) {
    if ('handleEvent' in listener) {
      // @ts-ignore
      listener.handleEvent(event);
    } else {
      // @ts-ignore
      listener(event);
    }
  }

  private _handleOpen = (event: Event) => {
    this._debug('open event');
    const { minUptime = DEFAULT.minUptime } = this._options;

    clearTimeout(this._connectTimeout);
    this._uptimeTimeout = setTimeout(() => this._acceptOpen(), minUptime);
    this._enterPhase('connected');

    // Only set binaryType in browser - ws package doesn't support 'blob'
    const isNode = typeof process !== 'undefined' && process.versions?.node;
    const isBun = typeof process !== 'undefined' && !!(process as any).isBun;
    if (!(isNode || isBun)) {
      this._ws!.binaryType = this._binaryType;
    }

    // send enqueued messages (messages sent before websocket open event)
    this._messageQueue.forEach((message) => this._ws?.send(message));
    this._messageQueue = [];

    if (this.onopen) {
      this.onopen(event);
    }
    this._listeners.open.forEach((listener) => this._callEventListener(event, listener));
  };

  private _handleMessage = (event: MessageEvent) => {
    this._debug('message event');

    if (this.onmessage) {
      this.onmessage(event);
    }
    this._listeners.message.forEach((listener) => this._callEventListener(event, listener));
  };

  private _handleError = (event: Events.ErrorEvent) => {
    this._debug('error event');
    this._disconnect(
      undefined,
      event.message === 'TIMEOUT' ? 'timeout' : undefined,
      event.message === 'TIMEOUT' ? 'local' : 'unknown',
    );

    if (this.onerror) {
      this.onerror(event);
    }
    this._debug('exec error listeners');
    this._listeners.error.forEach((listener) => this._callEventListener(event, listener));

    this._connect();
  };

  private _handleClose = (event: Events.CloseEvent) => {
    this._debug('close event');
    this._clearTimeouts();

    if (this._shouldReconnect) {
      if (this._options.reconnectsAtOnce?.(event)) this._retryCount = -1;
      this._connect();
    }

    if (this.onclose) {
      this.onclose(event);
    }
    this._listeners.close.forEach((listener) => this._callEventListener(event, listener));
  };

  private _removeListeners() {
    if (!this._ws) {
      return;
    }
    this._debug('removeListeners');
    this._ws.removeEventListener('open', this._handleOpen);
    this._ws.removeEventListener('close', this._handleClose);
    this._ws.removeEventListener('message', this._handleMessage);
    // @ts-ignore
    this._ws.removeEventListener('error', this._handleError);
  }

  private _addListeners() {
    if (!this._ws) {
      return;
    }
    this._debug('addListeners');
    this._ws.addEventListener('open', this._handleOpen);
    this._ws.addEventListener('close', this._handleClose);
    this._ws.addEventListener('message', this._handleMessage);
    // @ts-ignore
    this._ws.addEventListener('error', this._handleError);
  }

  private _clearTimeouts() {
    clearTimeout(this._connectTimeout);
    clearTimeout(this._uptimeTimeout);
  }
}
