// Hosted webhook listener client, shared by the standalone CLI and the web app:
// connects to news-gateway, registers as a webhook listener, and runs each
// delivered alert body through the host's `runBody`. Host specifics (token,
// identity, socket, how commands run) are injected; nothing here touches Node
// or the DOM. Design: docs/plans/WEBHOOK_RELAY_v1.md; frames:
// llt/news-gateway/CLAUDE.md.
//
// At-most-once is the gateway's ack gate, not ours: a delivery runs ONLY after
// `webhook.acked { status: 'received', accepted: true }` for its id. A refusal,
// a timeout or a dropped socket means it does not run here, ever.

export type WebhookPrintKind = 'print' | 'success' | 'warn' | 'error' | 'info';

/** What a delivered body runs with: an output sink and the `webhook` source tag. */
export interface WebhookRunContext {
  printSink: (kind: WebhookPrintKind, text: string) => void;
  source: 'webhook';
  /**
   * When the relay stops treating this alert as current. A host that cannot
   * start the body at once (the CLI queues it behind typed input) must refuse
   * to run it after this, so a stale alert never fires late.
   */
  expiresAt?: number;
}

export const DEFAULT_NEWS_WS_URL = 'wss://news.tealstreet.io/ws';
const ACK_TIMEOUT_MS = 10_000;
const REQUEST_TIMEOUT_MS = 5_000;
const RECONNECT_BASE_MS = 1_000;
const RECONNECT_MAX_MS = 30_000;
// Signed out or expired: re-read the token this often, so `login` in a running
// REPL brings webhooks back without a restart.
const SIGNED_OUT_RETRY_MS = 60_000;
const SEEN_DELIVERY_CAP = 500;
const REGISTER_RETRY_MS = 30_000;
const MAX_OUTPUT_CHARS = 16_000;
const MAX_ACCOUNTS = 100;

export interface RelaySocket {
  send(data: string): void;
  close(): void;
  onopen: (() => void) | null;
  onmessage: ((event: { data: unknown }) => void) | null;
  onclose: (() => void) | null;
  onerror: ((event: unknown) => void) | null;
}

/** Runs one delivered body as commands, queued behind whatever the user is typing. */
export type RunWebhookBody = (body: string, ctx: WebhookRunContext) => Promise<void>;

export interface RelayNotice {
  info(message: string): void;
  warn(message: string): void;
}

export interface WebhookRelayOptions {
  /** Stable per install (CLI) or per browser (web). */
  listenerId: string;
  kind: 'cli' | 'web';
  label: string;
  /** Account NAMES this listener can trade; re-read on every register. */
  getAccounts: () => string[];
  version?: string;
  /** The proxy-signed JWT news-gateway accepts in `rpc.auth`; null when signed out. */
  getToken: () => string | null | Promise<string | null>;
  url?: string;
  createSocket?: (url: string) => RelaySocket;
  runBody: RunWebhookBody;
  notice: RelayNotice;
  /** Mirrors a delivered chain's output somewhere the user can see it. */
  mirror?: (kind: WebhookPrintKind, text: string) => void;
  /** Called once a delivered chain has run, with the result it was acked with. */
  onFinished?: (result: { deliveryId: string; status: 'done' | 'failed'; output: string }) => void;
}

export interface WebhookRelayState {
  claimed_listener_id: string | null;
  hold_seconds: number;
  listeners: Array<{
    listener_id: string;
    kind: 'cli' | 'web';
    label: string;
    accounts: string[];
    online: boolean;
    last_seen: string;
  }>;
}

export type RelayConnection = 'off' | 'connecting' | 'connected' | 'unauthorized';

type Pending<T> = { resolve: (value: T) => void; timer: ReturnType<typeof setTimeout> };

const unrefTimer = (timer: ReturnType<typeof setTimeout>) => {
  (timer as { unref?: () => void }).unref?.();
  return timer;
};

export class WebhookRelayClient {
  private socket: RelaySocket | null = null;
  private stopped = false;
  private reconnectMs = RECONNECT_BASE_MS;
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  private currentToken: string | null = null;
  /** A token the gateway refused; never retried, only a NEW token (login) is. */
  private rejectedToken: string | null = null;
  private seen: string[] = [];
  /** Deliveries whose `received` ack is outstanding; a duplicate meanwhile is ignored. */
  private confirming = new Set<string>();
  /** Results not yet acknowledged by the relay; re-sent after every (re)register. */
  private unreportedResults = new Map<string, { status: 'done' | 'failed'; output: string }>();
  private registered = false;
  private registerRetryTimer: ReturnType<typeof setTimeout> | null = null;
  private seenSet = new Set<string>();
  private ackWaiters = new Map<string, Pending<boolean>>();
  private stateWaiters: Array<Pending<WebhookRelayState | null>> = [];
  private claimWaiters: Array<Pending<string>> = [];
  connection: RelayConnection = 'off';
  claimedListenerId: string | null = null;

  constructor(private readonly opts: WebhookRelayOptions) {}

  start(): void {
    this.stopped = false;
    this.connect();
  }

  stop(): void {
    this.stopped = true;
    if (this.reconnectTimer) clearTimeout(this.reconnectTimer);
    this.reconnectTimer = null;
    if (this.registerRetryTimer) clearTimeout(this.registerRetryTimer);
    this.registerRetryTimer = null;
    const socket = this.socket;
    this.socket = null;
    socket?.close();
    this.connection = 'off';
  }

  get listenerId(): string {
    return this.opts.listenerId;
  }

  private get selfName(): string {
    return this.opts.kind === 'cli' ? 'CLI' : 'browser';
  }

  get holdsClaim(): boolean {
    return this.claimedListenerId === this.listenerId;
  }

  private connect(): void {
    if (this.stopped) return;
    // A synchronous token (CLI config) connects synchronously; an async one
    // (the web app's token fetch) connects when it resolves.
    let pending: string | null | Promise<string | null>;
    try {
      pending = this.opts.getToken();
    } catch {
      pending = null;
    }
    if (pending instanceof Promise) {
      pending.then(
        (token) => this.connectWith(token),
        () => this.connectWith(null),
      );
      return;
    }
    this.connectWith(pending);
  }

  private connectWith(token: string | null): void {
    if (this.stopped) return;
    if (!token || token === this.rejectedToken) {
      this.connection = 'unauthorized';
      this.scheduleReconnect(SIGNED_OUT_RETRY_MS);
      return;
    }
    this.currentToken = token;
    const create = this.opts.createSocket ?? ((url: string) => new WebSocket(url) as unknown as RelaySocket);
    const socket = create(this.opts.url ?? DEFAULT_NEWS_WS_URL);
    this.socket = socket;
    this.connection = 'connecting';
    socket.onopen = () => this.sendFrame({ type: 'rpc.auth', token, source: this.opts.kind });
    socket.onmessage = (event) => this.onMessage(event.data);
    socket.onerror = () => {};
    socket.onclose = () => {
      if (this.socket !== socket) return;
      this.socket = null;
      this.failAllWaiters();
      this.registered = false;
      if (this.connection === 'unauthorized') {
        this.scheduleReconnect(SIGNED_OUT_RETRY_MS);
        return;
      }
      this.connection = 'connecting';
      this.scheduleReconnect();
    };
  }

  private scheduleReconnect(fixedDelay?: number): void {
    if (this.stopped || this.reconnectTimer) return;
    const delay = fixedDelay ?? this.reconnectMs;
    if (fixedDelay === undefined) this.reconnectMs = Math.min(this.reconnectMs * 2, RECONNECT_MAX_MS);
    this.reconnectTimer = unrefTimer(
      setTimeout(() => {
        this.reconnectTimer = null;
        this.connect();
      }, delay),
    );
  }

  private sendFrame(frame: Record<string, unknown>): boolean {
    if (!this.socket) return false;
    try {
      this.socket.send(JSON.stringify(frame));
      return true;
    } catch (err) {
      this.opts.notice.warn(`Webhooks: send failed: ${(err as Error).message}`);
      return false;
    }
  }

  private register(): void {
    const accounts = this.opts
      .getAccounts()
      .filter((name) => typeof name === 'string' && name.length > 0 && name.length <= 100)
      .slice(0, MAX_ACCOUNTS);
    this.sendFrame({
      type: 'webhook.register',
      listenerId: this.listenerId,
      kind: this.opts.kind,
      label: this.opts.label.slice(0, 200),
      accounts,
      ...(this.opts.version ? { version: this.opts.version.slice(0, 64) } : {}),
    });
  }

  private onMessage(raw: unknown): void {
    let frame: Record<string, unknown>;
    try {
      frame = JSON.parse(String(raw));
    } catch {
      return;
    }
    switch (frame.type) {
      case 'rpc.auth':
        if (frame.status === 'success') {
          this.reconnectMs = RECONNECT_BASE_MS;
          this.connection = 'connected';
          this.register();
        } else {
          // An expired/invalid token will not fix itself on retry; wait for a
          // new one (`login` writes it) instead of hammering with the old.
          this.rejectedToken = this.currentToken;
          this.connection = 'unauthorized';
          this.opts.notice.warn(
            this.opts.kind === 'cli'
              ? 'Webhooks: sign-in expired — run `login` to receive webhooks again.'
              : 'Webhooks: sign-in expired — sign in again to receive webhooks.',
          );
          this.socket?.close();
        }
        return;
      case 'webhook.registered': {
        this.registered = true;
        // A result lost with the previous socket is reported now; the relay
        // accepts it while the row is still `received`.
        for (const [deliveryId, result] of this.unreportedResults) {
          this.sendFrame({ type: 'webhook.ack', deliveryId, ...result });
        }
        // Silent: registering happens on every page load and reconnect, and is
        // not news. Only a claim CHANGE is announced (webhook.claim_changed).
        this.claimedListenerId = typeof frame.claimedListenerId === 'string' ? frame.claimedListenerId : null;
        return;
      }
      case 'webhook.claim_changed': {
        const before = this.holdsClaim;
        this.claimedListenerId = typeof frame.claimedListenerId === 'string' ? frame.claimedListenerId : null;
        if (this.holdsClaim && !before)
          this.opts.notice.info(`Webhooks: this ${this.selfName} now receives your TradingView alerts.`);
        if (!this.holdsClaim && before) {
          this.opts.notice.warn(
            this.claimedListenerId
              ? 'Webhooks: alerts now go to another listener (see `webhook status`).'
              : 'Webhooks: no listener is receiving alerts (run `webhook claim` to take them).',
          );
        }
        return;
      }
      case 'webhook.deliver':
        if (typeof frame.deliveryId === 'string' && typeof frame.body === 'string') {
          // A deadline on THIS clock, from the relay's remaining time, so a
          // skewed local clock can neither expire every alert nor none.
          const expiresAt =
            typeof frame.remainingMs === 'number' && Number.isFinite(frame.remainingMs)
              ? Date.now() + frame.remainingMs
              : undefined;
          void this.onDeliver(frame.deliveryId, frame.body, expiresAt);
        }
        return;
      case 'webhook.acked': {
        if (typeof frame.deliveryId !== 'string') return;
        if (frame.status === 'done' || frame.status === 'failed') {
          this.unreportedResults.delete(frame.deliveryId);
          return;
        }
        if (frame.status !== 'received') return;
        const waiter = this.ackWaiters.get(frame.deliveryId);
        if (!waiter) return;
        clearTimeout(waiter.timer);
        this.ackWaiters.delete(frame.deliveryId);
        waiter.resolve(frame.accepted === true);
        return;
      }
      case 'webhook.state': {
        const waiter = this.stateWaiters.shift();
        if (waiter) {
          clearTimeout(waiter.timer);
          waiter.resolve((frame.state as WebhookRelayState) ?? null);
        }
        return;
      }
      case 'webhook.claim_result': {
        const waiter = this.claimWaiters.shift();
        if (waiter) {
          clearTimeout(waiter.timer);
          waiter.resolve(typeof frame.status === 'string' ? frame.status : 'unknown');
        }
        return;
      }
      case 'webhook.error': {
        const message = typeof frame.message === 'string' ? frame.message : 'webhook error';
        if (frame.code === 'too_many_listeners') {
          this.opts.notice.warn(`Webhooks: ${message}`);
          return;
        }
        // A register the gateway could not complete leaves this socket deaf:
        // try again rather than reporting `connected` while receiving nothing.
        if (!this.registered && frame.code === 'unavailable') {
          this.scheduleRegisterRetry();
          return;
        }
        // The gateway lost this socket's registration while we believed it
        // held (a pod restart reusing the socket is not possible, but a
        // re-auth is): register again rather than going deaf.
        if (frame.code === 'not_registered' && this.registered && this.claimWaiters.length === 0) {
          this.registered = false;
          this.register();
          return;
        }
        // A request that failed answers its waiter instead of timing out.
        const claim = this.claimWaiters.shift();
        if (claim) {
          clearTimeout(claim.timer);
          claim.resolve(typeof frame.code === 'string' ? frame.code : 'error');
        } else {
          this.opts.notice.warn(`Webhooks: ${message}`);
        }
        return;
      }
      default:
        return;
    }
  }

  private scheduleRegisterRetry(): void {
    if (this.registerRetryTimer || this.stopped) return;
    this.registerRetryTimer = unrefTimer(
      setTimeout(() => {
        this.registerRetryTimer = null;
        if (this.connection === 'connected' && !this.registered) this.register();
      }, REGISTER_RETRY_MS),
    );
  }

  private markSeen(id: string): void {
    if (this.seenSet.has(id)) return;
    this.seenSet.add(id);
    this.seen.push(id);
    if (this.seen.length > SEEN_DELIVERY_CAP) {
      const dropped = this.seen.shift();
      if (dropped) this.seenSet.delete(dropped);
    }
  }

  private awaitReceivedAck(deliveryId: string): Promise<boolean> {
    return new Promise((resolve) => {
      const timer = unrefTimer(
        setTimeout(() => {
          this.ackWaiters.delete(deliveryId);
          resolve(false);
        }, ACK_TIMEOUT_MS),
      );
      this.ackWaiters.set(deliveryId, { resolve, timer });
    });
  }

  private async onDeliver(deliveryId: string, body: string, expiresAt?: number): Promise<void> {
    // A replay and a live NOTIFY can both deliver one alert; the ack gate
    // would refuse the second anyway, this just skips the round trip. Only an
    // ACCEPTED alert is remembered: one whose confirmation was lost must be
    // runnable when the relay replays it after a reconnect.
    if (this.seenSet.has(deliveryId) || this.confirming.has(deliveryId)) return;
    this.confirming.add(deliveryId);
    let accepted: boolean;
    try {
      const ack = this.awaitReceivedAck(deliveryId);
      if (!this.sendFrame({ type: 'webhook.ack', deliveryId, status: 'received' })) {
        const waiter = this.ackWaiters.get(deliveryId);
        if (waiter) {
          clearTimeout(waiter.timer);
          this.ackWaiters.delete(deliveryId);
        }
        return;
      }
      accepted = await ack;
    } finally {
      this.confirming.delete(deliveryId);
    }
    if (!accepted) {
      this.opts.notice.warn(`Webhooks: skipped an alert the relay did not confirm (${deliveryId.slice(0, 8)}).`);
      return;
    }
    this.markSeen(deliveryId);

    let output = '';
    let printedError = false;
    const printSink = (kind: WebhookPrintKind, text: string) => {
      if (kind === 'error') printedError = true;
      if (output.length < MAX_OUTPUT_CHARS) output += `${text}\n`;
      this.opts.mirror?.(kind, text);
    };
    let threw: string | null = null;
    try {
      await this.opts.runBody(body, { printSink, source: 'webhook', expiresAt });
    } catch (err) {
      threw = (err as Error).message;
      output += `${threw}\n`;
    }
    const status = threw || printedError ? 'failed' : 'done';
    const recorded = output.slice(0, MAX_OUTPUT_CHARS);
    // Kept until the relay acknowledges it, and re-sent on the next register,
    // so a socket lost during the run does not leave the row `received`.
    this.unreportedResults.set(deliveryId, { status, output: recorded });
    // Unregistered (mid-reconnect): the next webhook.registered sends it.
    if (this.registered) this.sendFrame({ type: 'webhook.ack', deliveryId, status, output: recorded });
    this.opts.onFinished?.({ deliveryId, status, output: recorded });
  }

  private failAllWaiters(): void {
    for (const [id, waiter] of this.ackWaiters) {
      clearTimeout(waiter.timer);
      waiter.resolve(false);
      this.ackWaiters.delete(id);
    }
    for (const waiter of this.stateWaiters.splice(0)) {
      clearTimeout(waiter.timer);
      waiter.resolve(null);
    }
    for (const waiter of this.claimWaiters.splice(0)) {
      clearTimeout(waiter.timer);
      waiter.resolve('disconnected');
    }
  }

  requestState(): Promise<WebhookRelayState | null> {
    return new Promise((resolve) => {
      if (!this.sendFrame({ type: 'webhook.state' })) return resolve(null);
      const waiter: Pending<WebhookRelayState | null> = {
        resolve,
        timer: unrefTimer(
          setTimeout(() => {
            this.stateWaiters = this.stateWaiters.filter((w) => w !== waiter);
            resolve(null);
          }, REQUEST_TIMEOUT_MS),
        ),
      };
      this.stateWaiters.push(waiter);
    });
  }

  private requestClaim(type: 'webhook.claim' | 'webhook.release'): Promise<string> {
    return new Promise((resolve) => {
      if (!this.sendFrame({ type })) return resolve('disconnected');
      const waiter: Pending<string> = {
        resolve,
        timer: unrefTimer(
          setTimeout(() => {
            this.claimWaiters = this.claimWaiters.filter((w) => w !== waiter);
            resolve('timeout');
          }, REQUEST_TIMEOUT_MS),
        ),
      };
      this.claimWaiters.push(waiter);
    });
  }

  claim(): Promise<string> {
    return this.requestClaim('webhook.claim');
  }

  release(): Promise<string> {
    return this.requestClaim('webhook.release');
  }
}
