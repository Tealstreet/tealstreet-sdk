/** The wasm-bindgen engine module (`pkg/web` or `pkg/node`, built by `yarn rust:build-wasm`). */
import type { EntropyProvider, WasmEngineLike } from './host.mjs';

export class WasmEngine implements WasmEngineLike {
  failed(): boolean;
  capabilities(): string;
  construction(): string;
  /** Native construction failures throw Error with JSON-encoded venueError detail. */
  constructor(venue: string, optionsJson: string, signerSetupJson?:string);
  /** Configure identity/ID and jitter entropy before startup; signing stays independent. */
  setEntropy(source: EntropyProvider): void;
  /** Explicit independent override for signing nonces and salts, before startup only. */
  setSigningEntropy(source: EntropyProvider): void;
  /** JSON-encoded health counters. */
  health(): string;
  start(now: number): string;
  command(json: string, now: number): string;
  takeFrame(now: number, marketOnly: boolean): Uint8Array | undefined;
  snapshotFrame(now: number): Uint8Array;
  accountSnapshotFrame(now: number): Uint8Array;
  wsOpen(socket: number, now: number): string;
  wsMessage(socket: number, text: string, now: number): string;
  /** Optional text fast paths; Paradex-only builds use wsMessage for controls. */
  wsMessageActions?(socket: number, text: string, now: number): string | undefined;
  wsMessageBytes?(socket: number, bytes: Uint8Array, now: number): string | undefined;
  wsBinary(socket: number, data: Uint8Array, now: number): string;
  wsClose(socket: number, code: number, now: number): string;
  wsRouteExhausted(socket: number, now: number): string;
  httpStart(request: number, status: number, headers: string, now: number): string;
  httpChunk(request: number, data: Uint8Array, now: number): string;
  httpEnd(request: number, now: number): string;
  httpFailed(request: number, error: string, now: number): string;
  signingPending(attempt: number): boolean;
  /** Present only in builds with the sign-meta feature. */
  signMeta?(attempt: number): string | undefined;
  httpPreparationPending(preparation: string): boolean;
  httpPrepared(preparation: string, resource: string, expiresAt: number | undefined, now: number): string;
  httpPreparationFailed(preparation: string, code: string, now: number): string;
  httpClaim(permit: string, now: number): string;
  httpGroupPending?(permit: string): boolean;
  httpClaimGroup?(permit: string, now: number, bounds: Float64Array): 0 | 1 | 2;
  /** Headers are name:value lines, with no added whitespace. */
  requiresExternalSigner(): boolean;
  /** JSON array of SignAlgorithm the JS provider can sign; omitted means unknown. */
  useExternalSigner(algorithmsJson?: string): void;
  hostedSigningClock(now: number): string;
  prepareHostedSigning(inputJson: string, now: number): string;
  localHostedSigning(id: number, now: number): string;
  finishHostedSigning(id: number, resultJson: string, now: number): string;
  cancelHostedSigning(id: number): void;
  signed(attempt: number, resultJson: string, now: number): string;
  signFailed(attempt: number, code: 'refused' | 'timedOut' | 'invalidResult' | 'unavailable', now: number): string;
  prepareDispatch(now: number): string;
  payloadSigned(request: number, signature: string, headers: string, now: number): string;
  payloadSignFailed(request: number, error: string, now: number): string;
  timer(timer: number, now: number): string;
  free(): void;
}

/** Bytes of wasm linear memory currently reserved. */
export function wasmMemoryBytes(): number;

/** Web build only: fetch and instantiate the module. */
export default function init(module?: unknown): Promise<unknown>;

/** Resolved account choices without construction or I/O. */
export function resolveConstruction(venue: string, optionsJson: string): string;

/** Kraken-family pure codec exports; absence is not capability negotiation. */
export function krakenHostedDescriptor(workJson: string): string;
export function krakenHostedValidate(descriptorJson: string, receiptJson: string, slot: string, roleJson: string, now: number, deadline: number): void;
export function krakenHostedVerify(descriptorJson: string, resultJson: string, publicCredential: string): void;
/** Trusted signing-only installation. Never pass setup through the op channel. */
export class HostedOperationExecutor {
  constructor(setupJson: string, spot: boolean, testnet: boolean);
  binding(slot: string): string;
  sign(descriptorJson: string, receiptJson: string, slot: string, roleJson: string, now: number, deadline: number): string;
  free(): void;
}
