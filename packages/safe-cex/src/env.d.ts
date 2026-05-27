/**
 * Minimal ambient Web API declarations for cross-runtime compatibility.
 * safe-cex runs in browsers, web workers, Bun, and Node.js.
 * These declarations allow TypeScript to typecheck browser-guarded code
 * without requiring "lib": ["dom"] in tsconfig.
 *
 * @types/node v25 provides some of these via web-globals (MessageEvent,
 * ErrorEvent, CryptoKey), but older versions and non-Node consumers
 * may not have them. This file ensures they are always available.
 */

// ─── Fix @types/node MessageEvent (re-export loses generic param) ────
// undici's MessageEvent<T> is generic but @types/node's re-export is not.
// Augment the global to restore the generic parameter.
declare interface MessageEvent<T = any> {
  readonly data: T;
}

// ─── Web API types that may be missing in older @types/node ──────────

// BroadcastChannel — available as a value in @types/node (worker_threads)
// but not as a global type. Declare the interface so it can be used as a type.
declare interface BroadcastChannel {
  readonly name: string;
  close(): void;
  postMessage(message: any): void;
  onmessage: ((this: BroadcastChannel, ev: MessageEvent) => any) | null;
  onmessageerror: ((this: BroadcastChannel, ev: MessageEvent) => any) | null;
}

// CryptoKey — used by coinbase.utils.ts for JWT signing
declare interface CryptoKey {
  readonly algorithm: object;
  readonly extractable: boolean;
  readonly type: string;
  readonly usages: string[];
}

// ErrorEvent — used in WebSocket error handlers
declare interface ErrorEvent {
  readonly message: string;
  readonly filename?: string;
  readonly lineno?: number;
  readonly colno?: number;
  readonly error?: any;
}

// CloseEvent — used by thirdparty/grvt-js-client
declare interface CloseEvent {
  readonly code: number;
  readonly reason: string;
  readonly wasClean: boolean;
}

// BinaryType — used by thirdparty/reconnecting-websocket
type BinaryType = 'blob' | 'arraybuffer';

// ─── Browser/Worker globals (may not exist at runtime) ───────────────

// window — browser main thread only
interface EthereumProvider {
  request(args: { method: string; params?: unknown[] }): Promise<unknown>;
  isMetaMask?: boolean;
  on?: (event: string, callback: (...args: unknown[]) => void) => void;
  removeListener?: (event: string, callback: (...args: unknown[]) => void) => void;
}

interface Window {
  ethereum?: EthereumProvider;
}
declare var window: Window | undefined;

// document — browser main thread only
interface Document {
  visibilityState?: string;
}
declare var document: Document | undefined;

// self — worker global scope (browser workers, SharedWorker)
interface WorkerGlobalScope {
  location: { origin: string; [key: string]: any };
  document?: unknown;
  postMessage(message: any, transfer?: Transferable[]): void;
  addEventListener(type: string, listener: (...args: any[]) => void): void;
  onconnect?: ((event: any) => void) | null;
}
declare var self: WorkerGlobalScope | undefined;

// sessionStorage — browser main thread only
interface Storage {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
  removeItem(key: string): void;
}
declare var sessionStorage: Storage | undefined;

// ─── Browser APIs (guarded with typeof checks) ──────────────────────

declare function requestAnimationFrame(callback: FrameRequestCallback): number;
type FrameRequestCallback = (time: number) => void;

declare class XMLHttpRequest {
  static readonly DONE: number;
  readonly status: number;
  readonly responseText: string;
  open(method: string, url: string, async?: boolean): void;
  send(body?: any): void;
  setRequestHeader(name: string, value: string): void;
  onload: ((this: XMLHttpRequest, ev: any) => any) | null;
  onerror: ((this: XMLHttpRequest, ev: any) => any) | null;
}

// Worker constructor (DOM Worker, not node:worker_threads)
declare class Worker {
  constructor(scriptURL: string | URL, options?: { type?: string; name?: string });
  postMessage(message: any, transfer?: Transferable[]): void;
  terminate(): void;
  onmessage: ((this: Worker, ev: MessageEvent) => any) | null;
  onerror: ((this: Worker, ev: ErrorEvent) => any) | null;
}
