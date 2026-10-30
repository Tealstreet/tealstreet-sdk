/** Runs a WasmEngine against a transport. See host.mjs. */
import type { EngineCapabilities } from './wire.mjs';

export type { EngineCapabilities };
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

export class HttpPreparationError extends Error {
  readonly code: string;
  constructor(reason: 'unsupported' | 'timedOut' | 'stale' | 'invalid' | 'network');
}

export interface HttpPacket {
  /** Required for managed requests; absent on older transport callers. */
  request?: number;
  method: string;
  route?: 'direct' | 'publicCors' | 'primary' | 'secondary';
  /** Remaining original lifecycle timeout, never renewed by binding. */
  timeoutMs?: number;
  url: string;
  headers: Record<string, string>;
  body?: string;
  signal?: AbortSignal;
}
export interface PreparedHttp {
  /** Optional absolute engine raw time. */
  expiresAt?: number;
  /** Validate/build synchronously; may not send or await. */
  bind(packet: HttpPacket): BoundHttp;
  /** Release only this prepared resource; idempotent. */
  cancel?(): void;
}
export interface BoundHttp {
  ready(): boolean;
  /** Invoke the transport API synchronously, return its response promise. */
  dispatch(): Promise<HttpResponse>;
  cancel?(): void;
}

export interface Transport {
  /** Default account HTTP adapter, resolved by the routing host. */
  httpMode?: 'direct' | 'how';
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
    handlers: { binary?: boolean; onBinary?(data: Uint8Array): void; context?: { feed: string; recover: boolean; rejectRoute?: boolean }; channel?: 'public' | 'private' | 'business'; headers?: Record<string, string>; onOpen(): void; onMessage(text: string): void; onClose(code?: number): void; onRouteExhausted?(): void },
  ): Socket;
  /** Explicit prepare/bind/claim support is mandatory for managed HTTP. */
  prepareHttp?(request: HttpPacket & { intent: 'public' | 'primary' | 'secondary' }): PreparedHttp | Promise<PreparedHttp>;
  request(request: HttpPacket): Promise<HttpResponse>;
  setTimeout(fn: () => void, ms: number): unknown;
  clearTimeout(handle: unknown): void;
}

/** Validated definitive GRVT builder refusal, independent of JS class identity. */
export interface BuilderAuthorizationDetail {
  code: 'GRVT_BUILDER_UNAUTHORIZED';
  reason: 'unauthorized' | 'cap-too-low';
  requiredRaw: number;
  authorizedRaw?: number;
}
export function builderAuthorizationOf(value: unknown): BuilderAuthorizationDetail | undefined;

/** Structured engine failure (`engine/src/error.rs`); `message` is the user-facing text. */
export interface VenueErrorDetail {
  message: string;
  kind?: 'rejected' | 'invalidInput' | 'insufficientBalance' | 'authInvalid' | 'authExpired' | 'authPermission'
    | 'rateLimited' | 'network' | 'timeout' | 'venueUnavailable' | 'unsupported' | 'internal';
  outcome?: 'notSent' | 'rejected' | 'unknown';
  venueCode?: string;
  venueMessage?: string;
  retryAfterMs?: number;
  partial?: { orderIds: string[]; total?: number; failedIndex?: number };
  builderAuthorization?: BuilderAuthorizationDetail;
}

export interface EngineHandlers {
  /** Called once after a WASM trap makes this host terminal and releases its I/O. */
  onDead?(error: EngineDeadError): void;
  onDeactivate?(): void;
  /** Typed native evidence for one configured signer and private-read scope.
   * No transport/body inference, command ownership or implicit error reporting. */
  onAccountAuth?(result: AccountAuthResult): void;
  /** Startup cannot complete; the account should fail `start` with this. */
  onStartupFailed?(detail: VenueErrorDetail): void;
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
  /** Structured events at or above the engine's `logLevel` option, plus semantic
   * history.incomplete markers regardless of level; Info+ also reach `onLog`
   * when this is absent. */
  onEvent?(event: EngineEvent): void;
  /** A socket opened, received its first frame, or closed; never per frame. */
  onSocketChange?(): void;
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
  readonly kind: 'internal';
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

/** A classified venue failure built from the engine's `detail`; `message` is unchanged venue text. */
export class ExchangeError extends Error {
  readonly code?: 'GRVT_BUILDER_UNAUTHORIZED';
  readonly reason?: BuilderAuthorizationDetail['reason'];
  readonly requiredRaw?: number;
  readonly authorizedRaw?: number;
  constructor(message: string, detail?: VenueErrorDetail);
  readonly kind?: NonNullable<VenueErrorDetail['kind']>;
  /** `unknown`: the write may have executed; `notSent`/`rejected`: it did not take effect. */
  readonly outcome?: NonNullable<VenueErrorDetail['outcome']>;
  readonly venueCode?: string;
  readonly venueMessage?: string;
  readonly retryAfterMs?: number;
  readonly partialOrderIds?: string[];
  readonly partialOrderCount?: number;
  readonly totalOrderCount?: number;
  readonly failedOrderIndex?: number;
  readonly receiptKey?: string;
  readonly commandId?: number;
  readonly orderIdentities?: readonly OrderIdentityReceipt[];
  readonly actualClientOrderIds?: readonly string[];
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

export type SignAlgorithm = 'hmacSha256' | 'hmacSha512' | 'sha256SecretSuffix' | 'ed25519' | 'es256' | 'secp256k1Digest' | 'starkDigest' | 'schnorrDigest' | 'zkLinkContract';
export type SignPart = { kind: 'bytes'; bytes: number[] } | { kind: 'credential'; key: string }
  | { kind: 'result'; index: number; encoding: 'raw' | 'hex' | 'base64' | 'base64Hex' | 'evmHex' };
export interface SigningIdentity {
  credential: string; signing: string; passphrase?: string;
  /** Trusted caller-assigned identity/nonce scope; never a key hash or slot name. */
  stableId?: string;
  /** Additional provider-owned signing refs; never an identity or nonce scope. */
  extra?: { zklinkSeed?: string; l1Wallet?: string };
}
export interface SigningIdentities { primary?: SigningIdentity; secondary?: SigningIdentity }
export interface LocalIdentityMetadata {
  stableId?: string;
  secretEncoding?: 'utf8' | 'base64' | 'hex' | 'pem' | 'base64Text' | 'base58';
  trimCredential?: boolean;
  /** Trim the signing-key text once at trusted setup. The SDK sets it only from
   * the family's declared policy; caller metadata cannot. */
  trimSigning?: boolean;
}
/** Trusted local setup metadata; credential strings arrive on the setup channel. */
export interface LocalSignerMetadata { primary?: LocalIdentityMetadata; secondary?: LocalIdentityMetadata }
/** Canonical facts in protocol UTC milliseconds; neither field is fabricated. */
export interface SignReceipt { stampMs?: number; expiryMs?: number; nonce?: string }
export interface SignResult {
  values: number[][];
  receipt: SignReceipt;
  /** Bounded same-attempt callback metadata; trusted native HTTP policy owns precedence. */
  headers?: [string, string][];
}

export interface HostedSigningClock { stampMs: number; revision: string }
export type HostedSigningInput = { kind: 'credential' }
  | { kind: 'http'; payload: string; stampMs: number; revision: string }
  | { kind: 'wsAuth'; toSign: string; outputFormat: 'hex' | 'base64' | 'hex-base64' | 'bitunix-double-sha256' };
export interface HostedSigningOutput { credential?: string; signature?: string; headers?: [string, string][] }
export interface SignRequest {
  receipt: SignReceipt;
  operations: ({ kind: 'sign'; algorithm: SignAlgorithm; key: string; payload: SignPart[] } | { kind: 'present'; key: string } | { kind: 'sha256'; payload: SignPart[] }
    | { kind: 'publicAddress'; key: string } | { kind: 'publicKey'; algorithm: SignAlgorithm; key: string })[];
  context: { venue: string; method: string; host: string; path: string };
}
export type SigningScope = 'account' | 'balance' | 'orders' | 'positions' | 'settings';
export type SigningRole = { readonly kind: 'read'; readonly scope: SigningScope | null }
  | { readonly kind: 'primaryRead'; readonly reason: 'clientOrderLookup' | 'postUnknownOrderRead'
    | 'degradedRead' | 'catalog' | 'history' | 'keyPermissions'; readonly scope: SigningScope | null }
  | { readonly kind: 'mutation' | 'sessionAuth' };
/** Immutable engine ownership captured at Action::Sign delivery. */
export interface SignMeta {
  readonly task: number;
  readonly owner: Readonly<{ kind: 'managed'; request: number; generation: number }
    | { kind: 'session'; generation: number }>;
  readonly deadlineMs: number;
  readonly slot: 'primary' | 'secondary';
  readonly role: SigningRole;
}
/** Owns secrets. Atomic results are exact raw operation bytes; errors are never logged. */
export type Signer = (request: SignRequest, context: { signal: AbortSignal; meta?: SignMeta }) => Promise<SignResult> | SignResult;

export interface EngineHostOptions {
  /** Transfer raw engine allocation ownership after successful construction.
   * Defaults false; borrowed callers dispose the host, then free their engine.
   * Trap cleanup is best effort. */
  ownsEngine?: boolean;
  /** Overrides local signing before startup. Context is advisory, not an allowlist. */
  signer?: Signer;
  /** What `signer` can sign. Undeclared is unknown, so multi-release commands
   * that check capability before their first release refuse as notSent. */
  signerAlgorithms?: readonly SignAlgorithm[];
  /** Synchronous identity/ID and retry-jitter bytes, also used for host receipt IDs.
   * Defaults to Web Crypto. A seed here never replaces signing entropy. */
  entropy?: EntropyProvider;
  /** Independent signing nonces/salts. Defaults to the engine's cryptographic source.
   * Deterministic signing requires this explicit override, intended for fixtures. */
  signingEntropy?: EntropyProvider;
  /** Whole-command waiting deadline, including pagination; default 120000 ms.
   * Positive safe integer <= 2147483647. Long history queries may need a larger value.
   * Expiry does not cancel venue work or retry it. */
  commandTimeoutMs?: number;
}

/** Return exactly length bytes; the host copies the returned storage.
 * Throwing or returning another type/length fails the draw. */
export type EntropyProvider = (length: number) => Uint8Array;

export interface CommandOptions {
  /** Override the host deadline for this call only; same bounds as commandTimeoutMs.
   * The effective wait also enters the engine as receipt metadata; never the venue. */
  timeoutMs?: number;
  /** Command-bound events while this request remains pending, including synchronous
   * dispatch. Internal fields.command is unchanged; routing hosts translate it.
   * Installed before dispatch and released on settlement, timeout or disposal.
   * Observer failures cannot replace the native result. */
  onEvent?(event: EngineEvent): void;
  /** Stamps this write's engine stages: `signed` only for a provider signer, and
   * `sent`/`response` for requests this command started. Observer failures are ignored. */
  onStage?(stage: 'accepted' | 'signed' | 'sent' | 'response' | 'reply'): void;
}

export interface EngineEvent {
  level: 'debug' | 'info' | 'warn' | 'error';
  /** Stable dotted identifier such as `ws.reconnect`; match on this, not the message. */
  code: string;
  message: string;
  /** Redacted values arrive as `"[redacted]"`; non-finite numbers as `null`. */
  fields: Record<string, string | number | boolean | null>;
}

/** One socket channel's state since host construction; times are `Date.now()`. */
export interface EngineSocketStats {
  /** Sockets currently open. */
  open: number;
  /** Open sockets that have received a frame since opening. */
  initialized: number;
  /** Newest frame on any socket of this channel; 0 before the first. */
  lastReceived: number;
  lastCloseCode?: number;
  lastCloseAt: number;
  /** Monotonic close count. */
  closes: number;
  /** Closes since a socket last received a frame. */
  closeStreak: number;
}

export type EngineSocketChannel = 'public' | 'business' | 'private';

/** Counters since engine construction. */
export interface EngineHealth {
  wsOpened: number;
  wsClosed: number;
  reconnects: number;
  authFailures: number;
  httpFailures: number;
  rateLimited: number;
  snapshotsDiscarded: number;
  rowsSkipped: number;
  /** Engine clock of the newest private-socket frame; null before the first. */
  lastPrivateMessage: number | null;
}

/** The subset of the wasm-bindgen engine the host drives. */
export interface EngineBridgeConstruction {
  spot: boolean;
  demo: boolean;
  primaryCredentialsOnly: boolean;
  payloadSigning: boolean;
  apiKeyIndexRequired: boolean;
  isolatedBookSubscriptions: boolean;
  dispatchSigning:boolean;
  /** Declared read-key policy; absent installs the read key as before. */
  readCredentials?: 'validateOnly' | 'publicRead';
}

export interface EngineConstruction {
  bridge?: EngineBridgeConstruction;
  canSign?: boolean;
  publicDataOnly?: boolean;
  asterdexApiVersion?: 'v1' | 'v3';
}

export interface WasmEngineLike {
  /** Sticky native fatal state; absent only on older or test engines. */
  failed?(): boolean;
  /** Optional native allocation cleanup; managed EngineHosts opt into ownership. */
  free?(): void;
  /** Configure before startup; optional only for older/fake engines without injection. */
  setEntropy?(source: EntropyProvider): void;
  setSigningEntropy?(source: EntropyProvider): void;
  /** Resolved account choices, without credentials. Older hosts may omit it. */
  construction?(): string;
  accountId?(): string | undefined;
  /** JSON-encoded {@link EngineHealth}. */
  health?(): string;
  /** JSON of the venue's capability record. */
  capabilities(): string;
  start(now: number): string;
  command(json: string, now: number): string;
  /** A `frameFed` engine's ingest of one data-source frame; returns actions JSON. */
  applyFrame?(frame: Uint8Array, snapshot: boolean, now: number): string;
  /** A `frameFed` engine's data-source clock minus the host's, in ms. */
  setSourceClockOffset?(offsetMs: number | undefined): void;
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
  wsRouteExhausted(socket: number, now: number): string;
  httpStart(request: number, status: number, headers: string, now: number): string;
  httpChunk(request: number, data: Uint8Array, now: number): string;
  httpEnd(request: number, now: number): string;
  httpFailed(request: number, error: string, now: number): string;
  signingPending?(attempt: number): boolean;
  /** Trusted pre-start negotiation; absent on builds without address-nonce. */
  setAddressSetupSupport?(protocol: 'publicAddressOwned'): void;
  initializeAddressSetup?(now: number, reinitialize: boolean): string;
  addressSetupReady?(): boolean;
  hostedWalletSigning?(toSign: string, outputFormat: string, now: number): string;
  addressSetupRetired?(token: string, now: number): string;
  /** Optional engine ownership export; invalid or absent data retains the legacy context. */
  signMeta?(attempt: number): string | undefined;
  httpPreparationPending?(preparation: string): boolean;
  httpPrepared?(preparation: string, resource: string, expiresAt: number | undefined, now: number): string;
  httpPreparationFailed?(preparation: string, code: 'unsupported' | 'timedOut' | 'stale' | 'invalid' | 'network', now: number): string;
  /** Scalar status only: never drain work before the synchronous handoff. */
  httpClaim?(permit: string, now: number): string;
  httpGroupPending?(permit: string): boolean;
  httpClaimGroup?(permit: string, now: number, bounds: Float64Array): 0 | 1 | 2;
  /** Headers are name:value lines, with no added whitespace. */
  requiresExternalSigner?(): boolean;
  useExternalSigner?(algorithmsJson?: string): void;
  hostedSigningClock?(now: number): string;
  prepareHostedSigning?(inputJson: string, now: number): string;
  localHostedSigning?(id: number, now: number): string;
  finishHostedSigning?(id: number, resultJson: string, now: number): string;
  cancelHostedSigning?(id: number): void;
  signed?(attempt: number, resultJson: string, now: number): string;
  signFailed?(attempt: number, code: 'refused' | 'timedOut' | 'invalidResult' | 'unavailable', now: number): string;
  prepareDispatch?(now: number): string;
  timer(timer: number, now: number): string;
}

export class EngineHost {
  readonly dead: boolean;
  readonly deadError?: EngineDeadError;
  /** The venue's capability record, read once from the engine. */
  capabilities(): EngineCapabilities;
  constructor(engine: WasmEngineLike, transport: Transport, handlers?: EngineHandlers, options?: EngineHostOptions);
  start(): void;
  /** Bind address-bound slots through the injected signer before `start`; resolves
   * once every setup task has settled (bound or refused). A no-op without a signer. */
  prepareAddressSetup(): Promise<void>;
  /** Sign compatibility bytes without startup, transport actions or account mutation. */
  hostedSigningClock(): HostedSigningClock;
  hostedSigning(input: HostedSigningInput): Promise<HostedSigningOutput>;
  /** Device answer for a keyless server's wallet ws-auth request, signed by the
   * local provider; refuses an injected signer. Returns the `0x` hex result. */
  hostedWalletSigning(input: { toSign: string; outputFormat: 'hyperliquid-l1' | 'hyperliquid-address' | 'lighter-tx' | 'paradex-stark' | 'paradex-public' }): string;
  /** Resolves with the command's result for commands that report one. */
  command(command: { op: string; [field: string]: unknown }, options?: CommandOptions): Promise<unknown>;
  /** With `marketOnly`, only books, candles and fills. */
  flush(marketOnly?: boolean): void;
  /** Apply one data-source frame to a `frameFed` engine; a snapshot resets its account state first. */
  applyFrame(frame: ArrayBuffer | Uint8Array, snapshot: boolean): void;
  /** A `frameFed` engine's data-source clock minus this host's, in ms. */
  setSourceClockOffset(offsetMs: number | undefined): void;
  snapshot(): ArrayBuffer;
  accountSnapshot(): ArrayBuffer;
  socketHealth(): Record<EngineSocketChannel, EngineSocketStats>;
  /** Abnormally close the open sockets of `channel` (all when omitted) so the
   * venue reconnects; returns how many were closed. */
  reconnectSockets(channel?: EngineSocketChannel): number;
  /** Undefined for an engine without `health()`. */
  health(): EngineHealth | undefined;
  dispose(): void;
}

export function nativeTransport(): Transport;

/** Ordered text and gzip/deflate binary WebSocket message decoding. */
export function socketMessages(onMessage: (text: string) => void, onError: (error: unknown) => void): (data: string | Blob | ArrayBuffer | ArrayBufferView) => void;

/** Preserve binary application frames when explicitly requested by the venue. */
export function socketMessages(onMessage: (data: string | Uint8Array) => void, onError: (error: unknown) => void, binary: boolean | undefined): (data: string | Blob | ArrayBuffer | ArrayBufferView) => void;
