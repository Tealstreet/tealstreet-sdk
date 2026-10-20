/** Runs a WasmEngine against a transport. See host.mjs. */
export const DEFAULT_COMMAND_TIMEOUT_MS: 120000;

export interface Socket {
  send(text: string): void;
  close(): void;
}

export interface HttpResponse {
  status: number;
  /** `name: value` lines. */
  headers?: string;
  body: Uint8Array | AsyncIterable<Uint8Array>;
}

export interface Transport {
  /** Default account HTTP adapter, resolved by the routing host. */
  httpMode?: 'direct' | 'how';
  signPayload?(request: {
    exchangeName: 'paradex'; key: string; payload: string; scheme: 'stark'; encoding: 'felt-pair';
  }, context: { signal: AbortSignal }): Promise<{ signature: string; headers?: Record<string, string> }> | { signature: string; headers?: Record<string, string> };
  /** Host-derived protocol options; apply after caller options when constructing the engine. */
  engineOptions?: Readonly<Record<string, unknown>>;
  /** Header-capable raw socket constructor for HOW and relay protocol clients. */
  WebSocket?: typeof globalThis.WebSocket;
  /** Server fetch does not add browser Origin/Referer headers automatically. */
  serverHeaders?: boolean;
  dispose?(): void;
  /** Upgrade headers must be supported without dropping authentication. */
  webSocketHeaders?: boolean;
  now(): number;
  connect(
    url: string,
    handlers: { binary?: boolean; onBinary?(data: Uint8Array): void; context?: { feed: string; recover: boolean }; channel?: 'public' | 'private' | 'business'; headers?: Record<string, string>; onOpen(): void; onMessage(text: string): void; onClose(code?: number): void },
  ): Socket;
  request(request: {
    method: string;
    /** Host policy; this is metadata, never an HTTP header or parameter. */
    route?: 'direct' | 'publicCors' | 'primary';
    /** Whole-operation deadline; defaults to 20 seconds. */
    timeoutMs?: number;
    url: string;
    headers: Record<string, string>;
    body?: string;
    /** Aborted on host disposal; check again after asynchronous authorization. */
    signal?: AbortSignal;
  }): Promise<HttpResponse>;
  setTimeout(fn: () => void, ms: number): unknown;
  clearTimeout(handle: unknown): void;
}

export interface EngineHandlers {
  /** Called once after a WASM trap makes this host terminal and releases its I/O. */
  onDead?(error: EngineDeadError): void;
  onDeactivate?(): void;
  /** Typed native evidence for one configured signer and private-read scope.
   * No transport/body inference, command ownership or implicit error reporting. */
  onAccountAuth?(result: AccountAuthResult): void;
  /** One frame per flush; transfer it, don't copy it. */
  onFrame?(buffer: ArrayBuffer): void;
  /** Associated native command errors arrive as warning diagnostics. Known
   * successful partial/best-effort commands promote the first diagnostic once
   * after settlement; rejected commands report at the SDK promise boundary.
   * Late diagnostics remain warnings. Background logs keep their native level. */
  onLog?(level: 'info' | 'warning' | 'error', message: string): void;
  /** Native identities are delivered before the corresponding I/O. Late events
   * retain the same key after a deadline; consumers may merge receipt tuples.
   * Complete events are idempotent and may repeat for duplicate native done. */
  onCommandProgress?(event: CommandProgress): void;
}

export interface AccountAuthResult {
  readonly credential: 'primary' | 'secondary';
  readonly scope: 'account' | 'balance' | 'orders' | 'positions' | 'settings';
  readonly outcome: 'accepted' | 'rejected';
}

export interface OrderIdentityReceipt {
  readonly symbol: string;
  readonly kind: 'clientOrderId' | 'clientOrderIndex' | 'orderCode';
  /** Exact native identity; numeric indices and codes remain decimal strings. */
  readonly identity: string;
}

export interface CommandProgress {
  readonly receiptKey: string;
  readonly commandId: number;
  /** Immutable per-action delta. The host keeps no receipt ledger after settlement. */
  readonly receipts: readonly OrderIdentityReceipt[];
  readonly complete: boolean;
  readonly late: boolean;
}

export class EngineDeadError extends Error {
  constructor(cause: unknown, command?: CommandIdentity);
  readonly code: 'ENGINE_DEAD';
  readonly command?: string;
  /** An interrupted write may already have been accepted; reconcile before retrying. */
  readonly outcome?: 'unknown';
  readonly clientOrderId?: string;
  readonly clientOrderIds?: string[];
  readonly receiptKey?: string;
  readonly commandId?: number;
  readonly orderIdentities?: readonly OrderIdentityReceipt[];
  readonly actualClientOrderIds?: readonly string[];
}

export interface CommandIdentity {
  op: string;
  /** Caller-supplied identifiers only; native-generated identifiers need separate receipts. */
  clientOrderId?: string;
  clientOrderIds?: string[];
  receiptKey?: string;
  commandId?: number;
  orderIdentities?: readonly OrderIdentityReceipt[];
}

export class CommandTimeoutError extends Error {
  constructor(timeoutMs: number, command?: CommandIdentity);
  readonly code: 'COMMAND_TIMEOUT';
  readonly timeoutMs: number;
  readonly command?: string;
  /** The write can still complete at the venue. Reconcile before retrying. */
  readonly outcome?: 'unknown';
  readonly clientOrderId?: string;
  readonly clientOrderIds?: string[];
  readonly receiptKey?: string;
  readonly commandId?: number;
  readonly orderIdentities?: readonly OrderIdentityReceipt[];
  readonly actualClientOrderIds?: readonly string[];
}

/** The host could not arm a deadline after handing the command to the engine.
 * This is a waiting failure, not an expired deadline or an engine trap. */
export class CommandDeadlineError extends Error {
  constructor(cause: unknown, command?: CommandIdentity);
  readonly code: 'COMMAND_DEADLINE_FAILED';
  readonly command?: string;
  readonly outcome?: 'unknown';
  readonly clientOrderId?: string;
  readonly clientOrderIds?: string[];
  readonly receiptKey?: string;
  readonly commandId?: number;
  readonly orderIdentities?: readonly OrderIdentityReceipt[];
  readonly actualClientOrderIds?: readonly string[];
}

/** A malformed result or unsupported action prevents confirming the command outcome. */
export class CommandProtocolError extends Error {
  constructor(cause: unknown, command?: CommandIdentity);
  readonly code: 'ENGINE_PROTOCOL_ERROR';
  readonly command?: string;
  readonly outcome?: 'unknown';
  readonly clientOrderId?: string;
  readonly clientOrderIds?: string[];
  readonly receiptKey?: string;
  readonly commandId?: number;
  readonly orderIdentities?: readonly OrderIdentityReceipt[];
  readonly actualClientOrderIds?: readonly string[];
}

export class EngineDisposedError extends Error {
  constructor(command?: CommandIdentity, interrupted?: boolean);
  readonly code: 'ENGINE_DISPOSED';
  readonly command?: string;
  readonly outcome?: 'unknown';
  readonly clientOrderId?: string;
  readonly clientOrderIds?: string[];
  readonly receiptKey?: string;
  readonly commandId?: number;
  readonly orderIdentities?: readonly OrderIdentityReceipt[];
  readonly actualClientOrderIds?: readonly string[];
}

export interface EngineHostOptions {
  /** Whole-command waiting deadline, including pagination; default 120000 ms.
   * Positive safe integer <= 2147483647. Long history queries may need a larger value.
   * Expiry does not cancel venue work or retry it. */
  commandTimeoutMs?: number;
}

export interface CommandOptions {
  /** Override the host deadline for this call only; same bounds as commandTimeoutMs.
   * Host metadata: never sent to the native engine or venue. */
  timeoutMs?: number;
}

/** The subset of the wasm-bindgen engine the host drives. */
export interface WasmEngineLike {
  accountId?(): string | undefined;
  start(now: number): string;
  command(json: string, now: number): string;
  takeFrame(now: number, marketOnly: boolean): Uint8Array | undefined;
  snapshotFrame(now: number): Uint8Array;
  /** Full private sections and unflushed catalog; absence is unsupported. */
  accountSnapshotFrame?(now: number): Uint8Array;
  wsOpen(socket: number, now: number): string;
  wsMessage(socket: number, text: string, now: number): string | undefined;
  /** Optional fast entry: undefined means no actions; legacy engines remain supported. */
  wsMessageActions?(socket: number, text: string, now: number): string | undefined;
  /** Optional UTF-8 entry; invalid bytes are ignored without changing engine state. */
  wsMessageBytes?(socket: number, bytes: Uint8Array, now: number): string | undefined;
  wsBinary?(socket: number, data: Uint8Array, now: number): string;
  wsClose(socket: number, code: number, now: number): string;
  httpStart(request: number, status: number, headers: string, now: number): string;
  httpChunk(request: number, data: Uint8Array, now: number): string;
  httpEnd(request: number, now: number): string;
  httpFailed(request: number, error: string, now: number): string;
  /** Headers are name:value lines, with no added whitespace. */
  payloadSigned?(request: number, signature: string, headers: string, now: number): string;
  payloadSignFailed?(request: number, error: string, now: number): string;
  timer(timer: number, now: number): string;
}

export class EngineHost {
  readonly dead: boolean;
  readonly deadError?: EngineDeadError;
  accountId(): string | undefined;
  constructor(engine: WasmEngineLike, transport: Transport, handlers?: EngineHandlers, options?: EngineHostOptions);
  start(): void;
  /** Resolves with the command's result for commands that report one. */
  command(command: { op: string; [field: string]: unknown }, options?: CommandOptions): Promise<unknown>;
  /** With `marketOnly`, only books, candles and fills. */
  flush(marketOnly?: boolean): void;
  snapshot(): ArrayBuffer;
  accountSnapshot(): ArrayBuffer;
  dispose(): void;
}

export function nativeTransport(): Transport;

/** Ordered text and gzip/deflate binary WebSocket message decoding. */
export function socketMessages(onMessage: (text: string) => void, onError: (error: unknown) => void): (data: string | Blob | ArrayBuffer | ArrayBufferView) => void;

/** Preserve binary application frames when explicitly requested by the venue. */
export function socketMessages(onMessage: (data: string | Uint8Array) => void, onError: (error: unknown) => void, binary: boolean | undefined): (data: string | Blob | ArrayBuffer | ArrayBufferView) => void;
