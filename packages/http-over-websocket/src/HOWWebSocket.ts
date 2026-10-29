import { nanoid } from 'nanoid';
import Emitter from 'tiny-emitter';

import { ReconnectingWebSocket } from '@tealstreet/reconnecting-websocket/src/ReconnectingWebsocket';

import { getUniqueLatencyDomains, syncServerTime } from './config';
import { decodeTradeTunnelMessage } from './tradeTunnelBinaryFrame';
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

export class HOWWebSocket {
  ws?: ReconnectingWebSocket;

  pingAt = 0;
  pingTimeoutId?: NodeJS.Timeout;
  isDisposed = false;
  isOpen = false;
  isInitialized = false;
  isTimeSynced = false;
  isTokenSent = false;
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

  // A getter is resolved at each connect, so the module-level singletons follow a
  // host app's `setTunnelEgressUrls` however early they were constructed.
  private url: string | (() => string);

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
    const options: Record<string, unknown> = {};

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
    ws = new ReconnectingWebSocket(wsUrl.toString(), protocols, options);
    ws.binaryType = 'arraybuffer';

    // Add error handler immediately to prevent uncaught errors
    ws.addEventListener('error', this.handleWebSocketError as any);

    // Also override onerror to catch any errors that might bypass addEventListener
    ws.onerror = this.handleWebSocketError as any;

    return ws;
  }

  protected handleWebSocketError = (event: Event) => {
    // Prevent the error from propagating
    event.preventDefault?.();
    event.stopPropagation?.();
    event.stopImmediatePropagation?.();

    // Extract error message if available
    const errorMessage = (event as any)?.message || (event as any)?.error?.message || (event as any)?.type;

    // Log WebSocket errors (state errors are now prevented in send() method)
    console.error(`WebSocket error: ${errorMessage || 'Unknown error'}`);
    return false;
  };

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

      // Create our own error with full context BEFORE the DOMException would occur
      const err = new Error(
        `HOWWebSocket${isTokenSend ? ' sendToken' : ''} not ready for send. State: ${stateName}, Message: ${message.substring(0, 100)}...`,
      );
      err.name = 'WebSocketNotReadyError';
      (err as any).webSocketState = stateName;
      (err as any).webSocketReadyState = wsState;
      (err as any).messagePreview = message.substring(0, 200);
      (err as any).fullMessage = message;
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

  connectAndSubscribe = () => {
    this.isInitialized = true;
    if (!this.isDisposed) {
      this.ws = this.createWebsocket(typeof this.url === 'function' ? this.url() : this.url);

      clearTimeout(this.lastMessageTimeout);

      this.ws.onopen = this.onOpen.bind(this);
      this.ws.onmessage = this.onMessage.bind(this);
      this.ws.onclose = this.onClose.bind(this);
    }
  };

  onOpen = async () => {
    this.epoch++;
    if (!this.isDisposed) {
      this.isOpen = true;

      this.pingTimeoutId = setTimeout(() => this.ping(), 5_000);

      // Start latency checks after a short delay to let connection stabilize
      this.startLatencyChecks();
    }
  };

  ping = () => {
    if (!this.isDisposed) {
      this.lastPingSentAt = Date.now();
      this.send({ type: 'rpc.ping', nonce: nanoid() });

      if (this.pingTimeoutId) {
        clearTimeout(this.pingTimeoutId);
      }
      this.pingTimeoutId = setTimeout(this.ping.bind(this), 5_000);
    }
  };

  sendToken = (data: any) => {
    this.epoch++;
    this.isTokenSent = true;
    this.authState = HOWWebSocketAuthState.Unknown;
    const message = JSON.stringify(data);
    this.validateAndSend(message, data, true);
  };

  onMessage = async ({ data: msgData }: MessageEvent): Promise<void> => {
    if (!this.isDisposed) {
      try {
        const json = await decodeTradeTunnelMessage(msgData);
        if (json?.type === 'rpc.auth') {
          this.epoch++;
          //{"type":"rpc.auth","error":"ExpiredSignature"}
          if (Boolean(json?.error)) {
            console.error(`[HOW] Tunnel auth failed: ${json.error}`);
            this.authState = HOWWebSocketAuthState.Bad;
          } else {
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

          clearTimeout(this.lastMessageTimeout);
          this.lastMessageTimeout = setTimeout(() => {
            if (!this.isDisposed) {
              this.ws?.reconnect?.();
            }
          }, 25_000);
        }
        if (json?.type === 'serverTime') {
          this.isTimeSynced = true;
          syncServerTime(json?.serverTime);
        }
        if (json?.type === 'rpc.http' && json?.nonce) {
          this.emit(`sub:${json?.nonce}`, json?.data || {});
        }
        if (json?.type === 'rpc.latency') {
          this.handleLatencyResponse(json);
        }
      } catch {}
    }
  };

  onClose = async () => {
    this.epoch++;
    this.isOpen = false;
    this.isTimeSynced = false;
    this.isTokenSent = false;
    this.authState = HOWWebSocketAuthState.Unknown;
    clearTimeout(this.lastMessageTimeout);
    console.error('[HOWWebSocket] WebSocket connection disconnected, reconnecting...');

    for (const event of this._activeEventHandlers) {
      console.error('[HOWWebSocket] emitting disconnect event for ', event);
      this.emit(event, { error: 'TradeTunnel connection lost, reconnecting...' });
    }

    if (this.pingTimeoutId) {
      clearTimeout(this.pingTimeoutId);
      this.pingTimeoutId = undefined;
    }

    // Stop latency checks
    this.stopLatencyChecks();
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

    this.latencyCheckTimeoutId = setTimeout(() => {
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
    this.ws?.close();
  };

  dispose = () => {
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

    this.ws?.terminate?.();
    this.ws = undefined;
    this._activeEventHandlers.clear();
  };
}
