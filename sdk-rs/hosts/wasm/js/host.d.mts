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
  /** Upgrade headers must be supported without dropping authentication. */
  webSocketHeaders?: boolean;
  now(): number;
  connect(
    url: string,
    handlers: { headers?: Record<string, string>; onOpen(): void; onMessage(text: string): void; onClose(code?: number): void },
  ): Socket;
  request(request: {
    method: string;
    url: string;
    headers: Record<string, string>;
    body?: string;
  }): Promise<HttpResponse>;
  setTimeout(fn: () => void, ms: number): unknown;
  clearTimeout(handle: unknown): void;
}

export interface EngineHandlers {
  /** One frame per flush; transfer it, don't copy it. */
  onFrame?(buffer: ArrayBuffer): void;
  onLog?(level: 'info' | 'warning' | 'error', message: string): void;
}

/** The subset of the wasm-bindgen engine the host drives. */
export interface WasmEngineLike {
  start(now: number): string;
  command(json: string, now: number): string;
  takeFrame(now: number, marketOnly: boolean): Uint8Array | undefined;
  snapshotFrame(now: number): Uint8Array;
  wsOpen(socket: number, now: number): string;
  wsMessage(socket: number, text: string, now: number): string;
  wsMessages(batch: Uint8Array): string;
  wsClose(socket: number, code: number, now: number): string;
  httpStart(request: number, status: number, headers: string, now: number): string;
  httpChunk(request: number, data: Uint8Array, now: number): string;
  httpEnd(request: number, now: number): string;
  httpFailed(request: number, error: string, now: number): string;
  timer(timer: number, now: number): string;
}

export class EngineHost {
  constructor(engine: WasmEngineLike, transport: Transport, handlers?: EngineHandlers, options?: { batchMessages?: boolean });
  start(): void;
  /** Resolves with the command's result for commands that report one. */
  command(command: { op: string; [field: string]: unknown }): Promise<unknown>;
  /** With `marketOnly`, only books, candles and fills. */
  flush(marketOnly?: boolean): void;
  snapshot(): ArrayBuffer;
  drainMessages(): void;
  dispose(): void;
}

export function nativeTransport(): Transport;
