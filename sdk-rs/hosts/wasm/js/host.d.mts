/** Runs a WasmEngine against a transport. See host.mjs. */
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
  onDeactivate?(): void;
  /** One frame per flush; transfer it, don't copy it. */
  onFrame?(buffer: ArrayBuffer): void;
  onLog?(level: 'info' | 'warning' | 'error', message: string): void;
}

/** The subset of the wasm-bindgen engine the host drives. */
export interface WasmEngineLike {
  accountId?(): string | undefined;
  start(now: number): string;
  command(json: string, now: number): string;
  takeFrame(now: number, marketOnly: boolean): Uint8Array | undefined;
  snapshotFrame(now: number): Uint8Array;
  wsOpen(socket: number, now: number): string;
  wsMessage(socket: number, text: string, now: number): string;
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
  timer(timer: number, now: number): string;
}

export class EngineHost {
  accountId(): string | undefined;
  constructor(engine: WasmEngineLike, transport: Transport, handlers?: EngineHandlers);
  start(): void;
  /** Resolves with the command's result for commands that report one. */
  command(command: { op: string; [field: string]: unknown }): Promise<unknown>;
  /** With `marketOnly`, only books, candles and fills. */
  flush(marketOnly?: boolean): void;
  snapshot(): ArrayBuffer;
  dispose(): void;
}

export function nativeTransport(): Transport;

/** Ordered text and gzip/deflate binary WebSocket message decoding. */
export function socketMessages(onMessage: (text: string) => void, onError: (error: unknown) => void): (data: string | Blob | ArrayBuffer | ArrayBufferView) => void;

/** Preserve binary application frames when explicitly requested by the venue. */
export function socketMessages(onMessage: (data: string | Uint8Array) => void, onError: (error: unknown) => void, binary: boolean | undefined): (data: string | Blob | ArrayBuffer | ArrayBufferView) => void;
