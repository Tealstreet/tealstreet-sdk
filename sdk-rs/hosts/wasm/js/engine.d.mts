/** The wasm-bindgen engine module (`pkg/web` or `pkg/node`, built by `yarn rust:build-wasm`). */
import type { WasmEngineLike } from './host.mjs';

export class WasmEngine implements WasmEngineLike {
  /** Available in builds with Coinbase portfolio selection. */
  accountId?(): string | undefined;
  constructor(venue: string, optionsJson: string);
  /** JSON-encoded health counters. */
  health(): string;
  start(now: number): string;
  command(json: string, now: number): string;
  takeFrame(now: number, marketOnly: boolean): Uint8Array | undefined;
  snapshotFrame(now: number): Uint8Array;
  wsOpen(socket: number, now: number): string;
  wsMessage(socket: number, text: string, now: number): string;
  /** Optional text fast paths; Paradex-only builds use wsMessage for controls. */
  wsMessageActions?(socket: number, text: string, now: number): string | undefined;
  wsMessageBytes?(socket: number, bytes: Uint8Array, now: number): string | undefined;
  wsBinary(socket: number, data: Uint8Array, now: number): string;
  wsClose(socket: number, code: number, now: number): string;
  httpStart(request: number, status: number, headers: string, now: number): string;
  httpChunk(request: number, data: Uint8Array, now: number): string;
  httpEnd(request: number, now: number): string;
  httpFailed(request: number, error: string, now: number): string;
  /** Headers are name:value lines, with no added whitespace. */
  payloadSigned(request: number, signature: string, headers: string, now: number): string;
  payloadSignFailed(request: number, error: string, now: number): string;
  timer(timer: number, now: number): string;
  free(): void;
}

/** Bytes of wasm linear memory currently reserved. */
export function wasmMemoryBytes(): number;

/** Web build only: fetch and instantiate the module. */
export default function init(module?: unknown): Promise<unknown>;
