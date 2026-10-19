/** The wasm-bindgen engine module (`pkg/web` or `pkg/node`, built by `yarn rust:build-wasm`). */
import type { WasmEngineLike } from './host.mjs';

export class WasmEngine implements WasmEngineLike {
  constructor(venue: string, optionsJson: string);
  start(now: number): string;
  command(json: string, now: number): string;
  takeFrame(now: number): Uint8Array | undefined;
  snapshotFrame(now: number): Uint8Array;
  wsOpen(socket: number, now: number): string;
  wsMessage(socket: number, text: string, now: number): string;
  wsClose(socket: number, code: number, now: number): string;
  httpStart(request: number, status: number, headers: string, now: number): string;
  httpChunk(request: number, data: Uint8Array, now: number): string;
  httpEnd(request: number, now: number): string;
  httpFailed(request: number, error: string, now: number): string;
  timer(timer: number, now: number): string;
  free(): void;
}

/** Bytes of wasm linear memory currently reserved. */
export function wasmMemoryBytes(): number;

/** Web build only: fetch and instantiate the module. */
export default function init(module?: unknown): Promise<unknown>;
