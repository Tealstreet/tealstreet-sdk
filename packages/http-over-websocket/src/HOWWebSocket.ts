import { nanoid } from 'nanoid';
import Emitter from 'tiny-emitter';

import { ReconnectingWebSocket } from '@tealstreet/reconnecting-websocket/src/ReconnectingWebsocket';

import { getUniqueLatencyDomains, syncServerTime } from './config';
import { diagnosticUrl, stripDiagnosticUrlQueries } from './diagnosticUrl';
import { decodeTradeTunnelMessage } from './tradeTunnelBinaryFrame';
import { noteTunnelDialFailed, noteTunnelDialOpened, tunnelDialUrl } from './tunnelEndpoints';
import { ConnectionDiagnostics, HiddenTimerLateness } from './connectionDiagnostics';

import { HOWWebSocketAuthState, LatencyEntry } from './types';

// Import ws package for Node.js/Bun - needed because Bun's native WebSocket
// doesn't support ALPNProtocols which is required to force HTTP/1.1
// (WebSocket upgrade doesn't work over HTTP/2)
let NodeWebSocket: typeof WebSocket | undefined;
const isNodeOrBun = typeof process !== 'undefined' && (process.versions?.node || (process as any).isBun);
if (isNodeOrBun) {
  try {
    // Dynamic import to avoid bundling ws in browser builds

    NodeWebSocket = require('ws');
  } catch {
    // ws not available, will fall back to global WebSocket
  }
}

// Latency check interval: 1 second tick, only query if data is stale (>10s old)
const LATENCY_CHECK_TICK = 1_000;
const LATENCY_STALE_THRESHOLD = 10_000;

// Extract domain from URL
const extractDomainFromUrl = (url: string): string | null => {
  try {
    const parsed = new URL(url);
    return parsed.hostname;
  } catch {
    return null;
  }
};

const WAITING_REDIAL_INTERVAL_MS = 5_000;
const CONNECT_TIMEOUT_MS = 4_000;
const CONNECT_AUDIT_MARGIN_MS = 2_000;
const CONNECT_CHECK_TICK_MS = 1_000;

type WebSocketFailureEvent = {
  preventDefault?: () => void;
  stopPropagation?: () => void;
  stopImmediatePropagation?: () => void;
  message?: unknown;
  error?: unknown;
  type?: string;
};

export class HOWWebSocket {
  ws?: ReconnectingWebSocket;

  private localClose = false;
  private reportedReconnectFailure = false;
  readonly diagnostics = new ConnectionDiagnostics('HOW');
  readonly timerLateness = new HiddenTimerLateness();
  pingAt = 0;
  pingTimeoutId?: NodeJS.Timeout;
  isDisposed = false;
  isOpen = false;
  isInitialized = false;
  isTimeSynced = false;
  isTokenSent = false;
  tokenSentAt = 0;
  authState = HOWWebSocketAuthState.Unknown;

  lastGoodPong = +new Date();
  lastMessageTimeout?: NodeJS.Timeout;

  // Tunnel latency (client <-> tradetunnel round trip)
  tunnelLatency: number | undefined;
  private lastPingSentAt: number = 0;

  // Exchange latency tracking (tradetunnel <-> exchange)
  latencyByDomain: Map<string, LatencyEntry> = new Map();
  private latencyCheckTimeoutId?: NodeJS.Timeout;
  private knownLatencyDomains: Set<string> = new Set(); // All valid latency domains from metadata
  private usedDomains: Set<string> = new Set(); // Domains seen in rpc.http requests

  private _emitter: Emitter.TinyEmitter = new (Emitter as any)();

  retries = 0;
  /** Invalidates readiness across connection or authentication transitions. */
  epoch = 0;
  private lastWaitingRedialAt = 0;

  private _activeEventHandlers = new Set<string>();

  once: Emitter.TinyEmitter['once'] = (event, listener) => {
    this._activeEventHandlers.add(event);
    return this._emitter.once(event, listener);
  };

  off: Emitter.TinyEmitter['off'] = (event, listener) => {
    this._activeEventHandlers.delete(event);
    return this._emitter.off(event, listener);
  };

  emit: Emitter.TinyEmitter['emit'] = (event, ...args) => {
    this._activeEventHandlers.delete(event);
    return this._emitter.emit(event, ...args);
  };

  // A getter is resolved at each connect, so the module-level singletons follow
  // `setTunnelEgressUrls` however early they were constructed.
  private url: string | (() => string);
  // The url the current connect attempt dialed, and whether that attempt opened.
  private dialedUrl = '';
  private dialOpened = false;
  private dialStartedAt?: number;
  private connectCheckTimeout?: ReturnType<typeof setTimeout>;
  private lifecycleDocument?: Document;
  private lifecycleWindow?: Window;

  constructor(
    url: string | (() => string),
    private readonly WebSocketCtor?: typeof WebSocket,
  ) {
    if (!url) {
      throw new Error('No HOW websocket endpoint provided');
    }
    this.url = url;
  }

  createWebsocket(url: string, protocols?: string | string[]) {
    let ws: ReconnectingWebSocket;

    // In Node.js/Bun, we need to:
    // 1. Use the ws package explicitly (Bun's native WebSocket doesn't support ALPNProtocols)
    // 2. Pass Origin header for CloudFront to accept the WebSocket upgrade
    // 3. Force HTTP/1.1 via ALPNProtocols (WebSocket upgrade doesn't work over HTTP/2)
    // The library's own default, stated so a reader (and the network lint) can see the bound.
    const options: Record<string, unknown> = { connectionTimeout: CONNECT_TIMEOUT_MS };

    const Socket = this.WebSocketCtor ?? NodeWebSocket;
    if (Socket) {
      options.WebSocket = Socket;
      options.headers = {
        Origin: 'https://preview.tealstreet.io',
        'User-Agent':
          'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/145.0.0.0 Safari/537.36',
      };
    }

    const wsUrl = new URL(url);
    wsUrl.searchParams.set('v', '2');
    wsUrl.searchParams.set('t', String(Date.now()));
    // Resolved per attempt, so a reconnect after the lane falls back dials 443.
    const dialUrl = () => {
      const dialedUrl = tunnelDialUrl(wsUrl.toString());
      // A disposed wrapper cannot update another connection's bookkeeping.
      if (this.ws === ws && ws.loopState.phase === 'connecting') {
        this.dialedUrl = dialedUrl;
        this.diagnostics.dial(dialedUrl);
        this.dialOpened = false;
        this.dialStartedAt = Date.now();
      }
      return dialedUrl;
    };
    ws = new ReconnectingWebSocket(dialUrl, protocols, options);
    ws.binaryType = 'arraybuffer';

    ws.onerror = this.handleWebSocketError;

    return ws;
  }

  protected handleWebSocketError = (event: WebSocketFailureEvent) => {
    // Prevent the error from propagating
    event.preventDefault?.();
    event.stopPropagation?.();
    event.stopImmediatePropagation?.();

    // Extract error message if available
    const errorMessage =
      typeof event.message === 'string'
        ? event.message
        : event.error instanceof Error
          ? event.error.message
          : event.type || 'Unknown error';
    // Node's ws exposes upgrade status here; browsers provide only an opaque error.
    const authRefusal = /^Unexpected server response: (401|403)$/.exec(errorMessage);
    if (authRefusal) {
      this.reportAuthFailure(`WebSocket upgrade refused (HTTP ${authRefusal[1]})`);
      return false;
    }

    this.reportReconnectFailures();
    // Log WebSocket errors (state errors are now prevented in send() method)
    console.info(
      `[HOWWebSocket] WebSocket error: ${stripDiagnosticUrlQueries(String(errorMessage || 'Unknown error'))}`,
    );
    return false;
  };

  private reportAuthFailure(error: string) {
    this.epoch++;
    this.authState = HOWWebSocketAuthState.Bad;
    console.error(`[HOW] Tunnel auth failed: ${error}`);
  }

  private reportReconnectFailures() {
    if (this.isDisposed || this.ws?.loopState.phase === 'stopped') return;
    const retries = this.ws?.retryCount ?? 0;
    if (retries < 3) this.reportedReconnectFailure = false;
    else if (!this.isDisposed && !this.reportedReconnectFailure) {
      this.reportedReconnectFailure = true;
      console.error('[HOWWebSocket] Repeated connection failures; retrying');
    }
  }

  private validateAndSend(message: string, data: any, isTokenSend = false, redacted = false): boolean {
    // Pre-check WebSocket state to avoid the opaque DOMException
    if (this.ws?.readyState !== WebSocket.OPEN) {
      if (redacted) throw new Error('Prepared HTTP tunnel connection is not open');
      const wsState = this.ws?.readyState;
      const stateNames = ['CONNECTING', 'OPEN', 'CLOSING', 'CLOSED'];
      const stateName = wsState !== undefined ? stateNames[wsState] : 'undefined';

      if (message.match(/(ping|pong)/i) || data?.type === 'rpc.ping') {
        // Silently ignore ping/pong when WebSocket is not OPEN
        return false;
      }

      const request = data?.arg ?? data;
      const preview = JSON.stringify({
        type: data?.type,
        method: request?.method,
        url: typeof request?.url === 'string' ? diagnosticUrl(request.url) : undefined,
      });
      const err = new Error(
        `HOWWebSocket${isTokenSend ? ' sendToken' : ''} not ready for send. State: ${stateName}, Message: ${preview}`,
      );
      err.name = 'WebSocketNotReadyError';
      (err as any).webSocketState = stateName;
      (err as any).webSocketReadyState = wsState;
      (err as any).messagePreview = preview;
      (err as any).isHOWWebSocket = true;
      if (isTokenSend) {
        (err as any).isReconnecting = !this.isOpen;
        (err as any).isTokenSend = true;
      }

      throw err;
    }

    // Safe to call send - state is OPEN
    this.ws.send(message);
    return true;
  }

  send = (data: any) => {
    const message = JSON.stringify(data);
    this.validateAndSend(message, data, false);

    // Track domains from rpc.http requests for latency monitoring
    if (data?.type === 'rpc.http' && data?.arg?.url) {
      this.trackDomainFromUrl(data.arg.url);
    }
  };

  /** Already validated/serialized at binding; no callbacks before raw send. */
  sendPrepared = (message: string, url: string) => {
    this.validateAndSend(message, { type: 'rpc.http' }, false, true);
    this.trackDomainFromUrl(url);
  };

  /**
   * Redials now when the reconnect loop is waiting out a backoff (after a wake or a
   * network drop) while a request needs the socket; at most once per 5 s per socket.
   */
  redialIfBackingOff = (): boolean => {
    if (this.isDisposed || this.ws?.loopState.phase !== 'retry-scheduled') return false;
    if (Date.now() - this.lastWaitingRedialAt < WAITING_REDIAL_INTERVAL_MS) return false;
    this.lastWaitingRedialAt = Date.now();
    this.ws.reconnect();
    return true;
  };

  /** Timer delivery is not a deadline: check elapsed wall time whenever JS runs. */
  redialIfConnectExpired = (): boolean => {
    const socket = this.ws;
    const loop = socket?.loopState;
    if (this.isDisposed || this.isOpen || !socket || loop?.phase !== 'connecting') return false;
    const startedAt = this.dialStartedAt ?? loop.since;
    if (socket.readyState === socket.OPEN || Date.now() - startedAt <= CONNECT_TIMEOUT_MS + CONNECT_AUDIT_MARGIN_MS)
      return false;

    // Native timeout wins when timers run normally; recovery keeps its retry count.
    // onClose counts a failed handshake only if this attempt actually obtained a URL.
    return socket.timeoutConnecting();
  };

  private checkConnectOnVisibility = () => {
    if (this.lifecycleDocument?.visibilityState === 'visible') this.redialIfConnectExpired();
  };

  private checkConnectOnResume = () => {
    this.redialIfConnectExpired();
  };

  private scheduleConnectCheck = () => {
    if (
      this.isDisposed ||
      this.isOpen ||
      !this.ws ||
      this.ws.loopState.phase === 'stopped' ||
      this.connectCheckTimeout !== undefined
    )
      return;
    this.connectCheckTimeout = this.observedTimeout(() => {
      this.connectCheckTimeout = undefined;
      try {
        this.redialIfConnectExpired();
      } finally {
        this.scheduleConnectCheck();
      }
    }, CONNECT_CHECK_TICK_MS);
  };

  connectAndSubscribe = () => {
    if (!this.isDisposed) {
      this.ws = this.createWebsocket(typeof this.url === 'function' ? this.url() : this.url);

      clearTimeout(this.lastMessageTimeout);

      this.ws.onopen = this.onOpen.bind(this);
      this.ws.onmessage = this.onMessage.bind(this);
      this.ws.onclose = this.onClose.bind(this);
      this.isInitialized = true;

      // Next constant-folds typeof checks on window/document to true in worker chunks,
      // so a bare global there throws; read them off globalThis instead.
      const page = (globalThis as { document?: Document }).document;
      if (!this.lifecycleDocument && page?.addEventListener) {
        this.lifecycleDocument = page;
        page.addEventListener('visibilitychange', this.checkConnectOnVisibility);
        page.addEventListener('resume', this.checkConnectOnResume);
      }
      const browserWindow = (globalThis as { window?: Window }).window;
      if (!this.lifecycleWindow && browserWindow?.addEventListener) {
        this.lifecycleWindow = browserWindow;
        browserWindow.addEventListener('pageshow', this.checkConnectOnResume);
      }
      this.scheduleConnectCheck();
    }
  };

  onOpen = async () => {
    this.epoch++;
    this.localClose = false;
    this.diagnostics.opened(true);
    this.dialOpened = true;
    noteTunnelDialOpened(this.dialedUrl);
    if (!this.isDisposed) {
      this.isOpen = true;
      // A frozen browser can redial a CLOSED socket before its old close event runs.
      // Never carry clock/auth readiness into a replacement connection.
      this.isTimeSynced = false;
      this.isTokenSent = false;
      this.tokenSentAt = 0;
      this.authState = HOWWebSocketAuthState.Unknown;
      clearTimeout(this.connectCheckTimeout);
      this.connectCheckTimeout = undefined;

      this.pingTimeoutId = this.observedTimeout(() => this.ping(), 5_000);
      // A socket that opens and then never pongs must be redialed too, not only one that stops.
      this.armSilenceReconnect();

      // Start latency checks after a short delay to let connection stabilize
      this.startLatencyChecks();
    }
  };

  private observedTimeout = (callback: () => void, delay: number): ReturnType<typeof setTimeout> => {
    const dueAt = Date.now() + delay;
    return setTimeout(() => {
      this.timerLateness.observe(dueAt);
      callback();
    }, delay);
  };

  private armSilenceReconnect = () => {
    clearTimeout(this.lastMessageTimeout);
    this.lastMessageTimeout = this.observedTimeout(() => {
      if (!this.isDisposed) {
        this.ws?.retryConnection();
      }
    }, 25_000);
  };

  ping = () => {
    if (!this.isDisposed) {
      this.lastPingSentAt = Date.now();
      this.send({ type: 'rpc.ping', nonce: nanoid() });

      if (this.pingTimeoutId) {
        clearTimeout(this.pingTimeoutId);
      }
      this.pingTimeoutId = this.observedTimeout(this.ping.bind(this), 5_000);
    }
  };

  sendToken = (data: any) => {
    this.epoch++;
    this.isTokenSent = true;
    this.tokenSentAt = Date.now();
    this.authState = HOWWebSocketAuthState.Unknown;
    const message = JSON.stringify(data);
    this.validateAndSend(message, data, true);
  };

  onMessage = async ({ data: msgData }: MessageEvent): Promise<void> => {
    if (!this.isDisposed) {
      try {
        const json = await decodeTradeTunnelMessage(msgData);
        if (json?.type === 'rpc.auth') {
          //{"type":"rpc.auth","error":"ExpiredSignature"}
          if (Boolean(json?.error)) {
            this.reportAuthFailure(String(json.error));
          } else {
            this.epoch++;
            this.authState = HOWWebSocketAuthState.Good;
          }
        }
        if (json?.type === 'heartbeat') {
          this.send({ type: 'rpc.ping', nonce: nanoid() });
        }
        if (json?.type === 'rpc.ping' && json?.status === 'success') {
          this.lastGoodPong = +new Date();

          // Calculate tunnel latency from ping/pong round trip
          if (this.lastPingSentAt > 0) {
            const newTunnelLatency = Date.now() - this.lastPingSentAt;
            // Use exponential moving average (alpha=0.3) for smoothing
            if (this.tunnelLatency === undefined) {
              this.tunnelLatency = newTunnelLatency;
            } else {
              this.tunnelLatency = Math.round(0.3 * newTunnelLatency + 0.7 * this.tunnelLatency);
            }
          }

          this.armSilenceReconnect();
        }
        if (json?.type === 'serverTime') {
          this.isTimeSynced = true;
          syncServerTime(json?.serverTime);
        }
        if (this.isTimeSynced && this.authState === HOWWebSocketAuthState.Good) this.diagnostics.established();
        if (json?.type === 'rpc.http' && json?.nonce) {
          this.emit(`sub:${json?.nonce}`, json?.data || {});
        }
        if (json?.type === 'rpc.latency') {
          this.handleLatencyResponse(json);
        }
      } catch {}
    }
  };

  onClose = async (event?: { code?: number; initiator?: 'local' | 'remote' | 'unknown' }) => {
    this.diagnostics.closed(
      event?.initiator ??
        (this.localClose ? 'local' : event?.code !== undefined && event.code !== 1006 ? 'remote' : 'unknown'),
    );
    this.epoch++;
    if (!this.dialOpened && this.dialedUrl && !this.isDisposed) noteTunnelDialFailed(this.dialedUrl);
    this.dialedUrl = '';
    this.dialOpened = false;
    this.dialStartedAt = undefined;
    this.isOpen = false;
    this.isTimeSynced = false;
    this.isTokenSent = false;
    this.authState = HOWWebSocketAuthState.Unknown;
    clearTimeout(this.lastMessageTimeout);
    if (!this.isDisposed) {
      if (event?.code === 4000 && this.ws?.loopState.phase === 'stopped') {
        console.error('[HOWWebSocket] Reconnect retries exhausted');
      } else {
        this.reportReconnectFailures();
        console.info('[HOWWebSocket] WebSocket connection disconnected, reconnecting...');
      }
    }

    for (const event of this._activeEventHandlers) {
      console.info('[HOWWebSocket] emitting disconnect event');
      this.emit(event, { error: 'TradeTunnel connection lost, reconnecting...' });
    }

    if (this.pingTimeoutId) {
      clearTimeout(this.pingTimeoutId);
      this.pingTimeoutId = undefined;
    }

    // Stop latency checks
    this.stopLatencyChecks();
    if (!this.isDisposed) this.scheduleConnectCheck();
  };

  // ==================== Latency Tracking ====================

  /**
   * Start periodic latency checks
   */
  startLatencyChecks = () => {
    // Initialize known domains from exchangeMetadata
    this.knownLatencyDomains = new Set(getUniqueLatencyDomains());

    if (this.knownLatencyDomains.size === 0) {
      return;
    }

    // Start the tick loop (will only query domains that have been used)
    this.scheduleNextLatencyTick();
  };

  /**
   * Stop latency checks and clear timeout
   */
  stopLatencyChecks = () => {
    if (this.latencyCheckTimeoutId) {
      clearTimeout(this.latencyCheckTimeoutId);
      this.latencyCheckTimeoutId = undefined;
    }
  };

  /**
   * Track a domain from an rpc.http request URL
   */
  private trackDomainFromUrl = (url: string) => {
    const domain = extractDomainFromUrl(url);
    if (!domain) return;

    // Only track if it's a known latency domain
    if (this.knownLatencyDomains.has(domain) && !this.usedDomains.has(domain)) {
      this.usedDomains.add(domain);

      // Query latency for this new domain immediately
      this.queryLatencyForDomain(domain);
    }
  };

  /**
   * Query latency for a specific domain
   */
  private queryLatencyForDomain = (domain: string) => {
    if (this.isDisposed || !this.isOpen) return;

    try {
      this.send({
        type: 'rpc.latency',
        nonce: nanoid(),
        domain,
      });
    } catch {
      // Silently ignore latency query errors
    }
  };

  /**
   * Schedule the next latency tick (runs every 1 second)
   */
  private scheduleNextLatencyTick = () => {
    if (this.isDisposed || !this.isOpen) {
      return;
    }

    this.latencyCheckTimeoutId = this.observedTimeout(() => {
      this.queryStaleLatencies();
      this.scheduleNextLatencyTick();
    }, LATENCY_CHECK_TICK);
  };

  /**
   * Check if a domain's latency data is stale (missing or older than threshold)
   */
  private isLatencyStale = (domain: string): boolean => {
    const entry = this.latencyByDomain.get(domain);
    if (!entry) {
      return true; // No data = stale
    }
    const age = Date.now() - entry.updatedAt;
    return age > LATENCY_STALE_THRESHOLD;
  };

  /**
   * Query latency only for used domains with stale data
   */
  private queryStaleLatencies = () => {
    if (this.isDisposed || !this.isOpen || this.usedDomains.size === 0) {
      return;
    }

    for (const domain of this.usedDomains) {
      if (this.isLatencyStale(domain)) {
        try {
          this.send({
            type: 'rpc.latency',
            nonce: nanoid(),
            domain,
          });
        } catch {
          // Silently ignore latency query errors
        }
      }
    }
  };

  /**
   * Handle rpc.latency response from the server
   */
  private handleLatencyResponse = (json: any) => {
    const { domain, latency_ms, source, cached, sample_count, measured_at, error } = json;

    if (error) {
      return;
    }

    if (!domain || latency_ms === undefined) {
      return;
    }

    const entry: LatencyEntry = {
      latencyMs: latency_ms,
      source: source || 'unknown',
      cached: cached ?? false,
      sampleCount: sample_count ?? 1,
      measuredAt: measured_at ?? Date.now(),
      updatedAt: Date.now(),
    };

    this.latencyByDomain.set(domain, entry);
  };

  /**
   * Get latency for a specific domain
   */
  getLatencyForDomain = (domain: string): LatencyEntry | undefined => {
    return this.latencyByDomain.get(domain);
  };

  /**
   * Get all latency entries
   */
  getAllLatencies = (): Map<string, LatencyEntry> => {
    return new Map(this.latencyByDomain);
  };

  /**
   * @deprecated debug only
   */
  simulateClose = () => {
    this.localClose = true;
    this.ws?.close();
  };

  dispose = () => {
    this.localClose = true;
    if (this.ws) this.diagnostics.closed('local');
    this.isDisposed = true;
    this.isOpen = false;
    this.isInitialized = false;
    this.isTimeSynced = false;
    this.isTokenSent = false;
    this.authState = HOWWebSocketAuthState.Unknown;

    if (this.pingTimeoutId) {
      clearTimeout(this.pingTimeoutId);
      this.pingTimeoutId = undefined;
    }
    clearTimeout(this.lastMessageTimeout);
    this.lastMessageTimeout = undefined;
    this.stopLatencyChecks();

    clearTimeout(this.connectCheckTimeout);
    this.connectCheckTimeout = undefined;
    this.lifecycleDocument?.removeEventListener('visibilitychange', this.checkConnectOnVisibility);
    this.lifecycleDocument?.removeEventListener('resume', this.checkConnectOnResume);
    this.lifecycleWindow?.removeEventListener('pageshow', this.checkConnectOnResume);
    this.lifecycleDocument = undefined;
    this.lifecycleWindow = undefined;

    this.ws?.terminate?.();
    this.ws = undefined;
    this._activeEventHandlers.clear();
  };
}
