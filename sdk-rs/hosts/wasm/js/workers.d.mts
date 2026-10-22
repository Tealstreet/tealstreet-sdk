import type { EngineHost, Signer, SigningIdentities, Transport, WasmEngineLike } from './host.mjs';

/** A header-capable socket implemented with Workers' fetch upgrade API. */
export const WorkersWebSocket: typeof globalThis.WebSocket;
export function workersTransport(): Transport;

/** Diagnostic messages are `{ op: "event", level, code, message, fields }`;
 * levels use "warn". Host diagnostics have host.* codes. */
export function createEngineObject<Venue extends string = string, Options = Record<string, unknown>>(configuration: {
  /** Resolve a deployment-managed bearer token for this object's account. Missing tokens deny upgrades. */
  accountToken(state: unknown, env: unknown): string | undefined | Promise<string | undefined>;
  loadEngine(venue: Venue): Promise<{ WasmEngine: new (venue: string, optionsJson: string) => WasmEngineLike }>;
  /** Trusted deployment provider; callbacks and keys never come from client messages. */
  createSigner?(state: unknown, env: unknown, venue: Venue):
    { signer: Signer; identities: SigningIdentities } | Promise<{ signer: Signer; identities: SigningIdentities }>;
  createTransport?(venue: Venue, options: Options, raw: Transport): Transport;
  flush?: { store: number; books: number };
}): new (state: { storage: { setAlarm(at: number): unknown } }, env?: unknown) => {
  host?: EngineHost;
  fetch(request: Request): Promise<Response>;
  receive(client: Pick<WebSocket, 'send'>, data: unknown): Promise<void>;
  leave(client: Pick<WebSocket, 'send'>): void;
  alarm(): Promise<void>;
};
