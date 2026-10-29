import type { SignAlgorithm, Signer, SignReceipt, SignRequest, SignResult } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeName, ExchangeOptions, PayloadSigner, RemoteSigner } from '../../types';
import type { RemoteSigningPolicy } from './construction';

import { sha256 } from '@noble/hashes/sha2.js';
import { nanoid } from 'nanoid';

import { deriveRemoteSigningPolicy } from './construction';

const HMAC_ALGORITHMS: readonly SignAlgorithm[] = Object.freeze(['hmacSha256']);
const DOUBLE_SHA256_ALGORITHMS: readonly SignAlgorithm[] = Object.freeze(['sha256SecretSuffix']);
const WALLET_ALGORITHMS: readonly SignAlgorithm[] = Object.freeze(['secp256k1Digest']);
const SCHNORR_ALGORITHMS: readonly SignAlgorithm[] = Object.freeze(['schnorrDigest']);
/** Families whose remote wire signs `hex(sha256(hex(sha256(toSign)) ‖ material))`. */
const doubleSha256 = (name: ExchangeName) => name === 'bitunix';
/** Families whose device re-derives a wallet digest from a structured statement. */
const hyperliquid = (name: ExchangeName) => name === 'hyperliquid' || name === 'hyperliquidspot';
/** Families whose device re-derives Schnorr signing words from a statement. */
const lighter = (name: ExchangeName) => name === 'lighter' || name === 'lighter-robinhood';
/** One Lighter request: the statement commitment and at most 31 signatures. */
const LIGHTER_SIGNATURE = 80;

/** Everything this provider signs for a venue; the engine's capability admission
 * reads it. A double-SHA256 family is never advertised an HMAC, nor the reverse. */
export const remoteSignerAlgorithms = (name: ExchangeName): readonly SignAlgorithm[] =>
  doubleSha256(name) ? DOUBLE_SHA256_ALGORITHMS : hyperliquid(name) ? WALLET_ALGORITHMS
    : lighter(name) ? SCHNORR_ALGORITHMS : HMAC_ALGORITHMS;

export interface RemoteSignerIdentity {
  credential: string;
  signing: string;
  publicCredential: string;
  passphrase?: string;
  /** Trusted setup input, consumed into Present-only provider custody. */
  passphraseValue?: string;
}

export interface RemoteSignerProviderOptions {
  exchangeName: ExchangeName;
  identities: { primary?: RemoteSignerIdentity; secondary?: RemoteSignerIdentity };
  remoteSigner?: RemoteSigner;
  payloadSigner?: PayloadSigner;
  /** Setup input is projected onto declared non-secret metadata, never retained. */
  payloadSignerMetadata?: Partial<ExchangeOptions>;
}

/** Metadata belongs only to the current validated signing attempt. */
export type RemoteSignerResult = SignResult & { headers?: [string, string][] };

const MAX_PAYLOAD = 1_048_576;
const MAX_OPERATIONS = 32;
const MAX_PARTS = 256;
// Native hosts can import the registry before installing their runtime codecs.
// Only actual provider work requires UTF-8; importing the SDK does not.
let encoder: TextEncoder | undefined;
let decoder: TextDecoder | undefined;
const encodeUtf8 = (value: string) => (encoder ??= new TextEncoder()).encode(value);
const decodeUtf8 = (value: Uint8Array) =>
  (decoder ??= new TextDecoder('utf-8', { fatal: true, ignoreBOM: true })).decode(value);
const refused = () => new Error('Remote signing request refused');
const canceled = () => new Error('Remote signing canceled');
const unavailable = () => new Error('Remote signer unavailable');
const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);

function utf8(value: string): Uint8Array {
  if (value.length > MAX_PAYLOAD) throw refused();
  const bytes = encodeUtf8(value);
  if (bytes.length > MAX_PAYLOAD || decodeUtf8(bytes) !== value) throw refused();
  return bytes;
}

function validRef(value: unknown): value is string {
  return typeof value === 'string' && value.length > 0 && value.length <= 256
    && utf8(value).length <= 256;
}

function mac(value: unknown, encoding: 'hex' | 'base64'): number[] {
  if (encoding === 'base64') {
    // A SHA-256 MAC has 32 bytes: exactly 43 data characters and one '='.
    // Nonzero unused pad bits are not a second spelling of the same MAC.
    if (typeof value !== 'string' || !/^[A-Za-z0-9+/]{43}=$/.test(value)) throw refused();
    const alphabet = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/';
    if ((alphabet.indexOf(value[42]) & 3) !== 0) throw refused();
    const bytes: number[] = [];
    let accumulator = 0;
    let bits = 0;
    for (const char of value.slice(0, -1)) {
      accumulator = (accumulator << 6) | alphabet.indexOf(char);
      bits += 6;
      if (bits >= 8) {
        bits -= 8;
        bytes.push((accumulator >>> bits) & 255);
      }
    }
    if (bytes.length !== 32) throw refused();
    return bytes;
  }
  if (typeof value !== 'string' || !/^[0-9a-fA-F]{64}$/.test(value)) throw refused();
  return Array.from({ length: 32 }, (_, i) => Number.parseInt(value.slice(i * 2, i * 2 + 2), 16));
}

function headers(value: unknown): [string, string][] | undefined {
  if (value === undefined) return undefined;
  if (!record(value)) throw refused();
  const entries = Object.entries(value);
  if (entries.length > 32) throw refused();
  let total = 0;
  const normalized = new Map<string, [string, string]>();
  for (const [name, item] of entries) {
    if (!/^[!#$%&'*+\-.^_`|~0-9A-Za-z]+$/.test(name) || name.length > 256
      || typeof item !== 'string' || /[\x00-\x08\x0a-\x1f\x7f]/.test(item)) throw refused();
    const size = utf8(item).length;
    total += name.length + size;
    if (size > 8192 || total > 32768) throw refused();
    // Axios treats aliases case-insensitively. Keep the last value and spelling.
    normalized.set(name.toLowerCase(), [name, item]);
  }
  return Array.from(normalized.values());
}

/** Cancel waiting without requiring either legacy callback to support AbortSignal.
 * Every subscription/listener belongs to this one attempt, including synchronous
 * availability changes during subscription and late callback completion.
 */
function guarded<T>(signal: AbortSignal, remote: RemoteSigner | undefined, invoke: () => T | Promise<T>): Promise<T> {
  return new Promise((resolve, reject) => {
    let settled = false;
    let unsubscribe: (() => void) | undefined;
    const cleanup = () => {
      signal.removeEventListener('abort', abort);
      const release = unsubscribe;
      unsubscribe = undefined;
      try { release?.(); } catch { /* Cleanup cannot expose callback errors. */ }
    };
    const finish = (error: Error | undefined, value?: T) => {
      if (settled) return;
      settled = true;
      cleanup();
      if (error) reject(error);
      else resolve(value as T);
    };
    const abort = () => finish(canceled());
    try {
      if (signal.aborted) return finish(canceled());
      signal.addEventListener('abort', abort, { once: true });
      if (remote) {
        if (!remote.isAvailable()) return finish(unavailable());
        unsubscribe = remote.onAvailabilityChange((available) => {
          if (!available) finish(unavailable());
        });
        if (typeof unsubscribe !== 'function') return finish(refused());
        // A synchronous notification may have settled before release was assigned.
        if (settled) { cleanup(); return; }
        if (!remote.isAvailable()) return finish(unavailable());
      }
      if (signal.aborted) return finish(canceled());
      Promise.resolve(invoke()).then(
        (value) => finish(signal.aborted ? canceled() : undefined, value),
        () => finish(refused()),
      );
    } catch {
      finish(refused());
    }
  });
}

/** Explicit projection avoids retaining secrets hidden in nested setup, callback,
 * transport or arbitrary caller objects. Each slot gets its selected public key.
 */
function payloadReceiver(options: Partial<ExchangeOptions> | undefined, key: string): Readonly<Record<string, unknown>> {
  const receiver: Record<string, unknown> = { key };
  for (const field of ['testnet', 'enableSpotMarkets', 'enableOptionMarkets', 'enableInverseMarkets',
    'bybitSiteId', 'publicDataOnly', 'privateOnly', 'engine', 'subaccount', 'hostedMode',
    'tealstreetProxy', 'tealstreetTunnel', 'tealstreetWsRoute', 'proxyRegion'] as const) {
    const value = options?.[field];
    if (typeof value === 'string' || typeof value === 'boolean') receiver[field] = value;
  }
  if (options?.extra) {
    const extra: Record<string, unknown> = {};
    for (const field of ['tickInterval', 'rustCommandTimeoutMs', 'recvWindow', 'hyrotraderMode'] as const) {
      const value = options.extra[field];
      if (typeof value === 'number' || typeof value === 'string') extra[field] = value;
    }
    receiver.extra = Object.freeze(extra);
  }
  return Object.freeze(receiver);
}

/** Decode only the declared prepared WS grammar; never use advisory context as
 * authority or draw a new timestamp. HTTP work keeps ordinary correlation ids.
 */
function wsMetadata(wire: RemoteSigningPolicy['wire'], payload: string, key: string, receipt: SignReceipt, credentialDelimited: boolean, suffixDelimited: boolean): Record<string, unknown> | undefined {
  const codec = wire?.extraData;
  const seconds = wire?.secondsSuffix;
  // Native WS recipes delimit the owned public key as a Credential part, or a
  // declared seconds suffix as typed parts; hosted WS captures have no stamp.
  if (receipt.stampMs !== undefined && !credentialDelimited && !suffixDelimited) return undefined;
  const stamp = (text: string) => /^(0|[1-9][0-9]*)$/.test(text) && Number.isSafeInteger(Number(text)) ? Number(text) : undefined;
  if (codec === 'secondsTimestamp' && seconds) {
    const suffix = `${seconds.method}${seconds.path}`;
    if (!payload.endsWith(suffix)) return undefined;
    const timestamp = stamp(payload.slice(0, -suffix.length));
    if (timestamp === undefined || !Number.isSafeInteger(timestamp * 1000)
      || (receipt.stampMs !== undefined && receipt.stampMs !== timestamp * 1000)) throw refused();
    return { timestamp };
  } else if (codec === 'bybitExpiry') {
    if (!payload.startsWith('GET/realtime')) return undefined;
    const expiry = stamp(payload.slice('GET/realtime'.length));
    if (expiry === undefined || (receipt.expiryMs !== undefined && receipt.expiryMs !== expiry)) throw refused();
    return { expires: expiry };
  } else if (codec === 'binanceSpot') {
    const prefix = `apiKey=${key}&recvWindow=`;
    if (!payload.startsWith(prefix)) return undefined;
    const tail = payload.slice(prefix.length).split('&timestamp=');
    const recvWindow = tail.length === 2 ? stamp(tail[0]) : undefined;
    const timestamp = tail.length === 2 ? stamp(tail[1]) : undefined;
    if (recvWindow === undefined || timestamp === undefined || (receipt.stampMs !== undefined && receipt.stampMs !== timestamp)) throw refused();
    return { apiKey: key, recvWindow, timestamp };
  }
  return undefined;
}

/** Bridge only trusted account-owned public identities to legacy HMAC callbacks.
 * Advisory SignRequest.context never establishes key or account authority.
 */
export function createRemoteSignerProvider(options: RemoteSignerProviderOptions): Signer {
  const { exchangeName, remoteSigner, payloadSigner } = options;
  const declaration = deriveRemoteSigningPolicy(exchangeName)?.wire;
  const wire = declaration && { ...declaration,
    ...(declaration.secondsSuffix && { secondsSuffix: { ...declaration.secondsSuffix } }) };
  const presentation = deriveRemoteSigningPolicy(exchangeName);
  const encoding = presentation?.signatureEncoding ?? 'hex';
  const passphraseHeader = presentation?.passphraseHeader;
  const algorithms = remoteSignerAlgorithms(exchangeName);
  let accountName: string | undefined;
  const owned = new Map<string, { identity: RemoteSignerIdentity; primary: boolean; receiver: Readonly<Record<string, unknown>> }>();
  const credentials = new Map<string, { bytes: number[]; identity: RemoteSignerIdentity }>();
  // Passphrases may be presented with owned signing work, never interpolated.
  const passphrases = new Map<string, number[]>();
  try {
    if (encoding !== 'hex' && encoding !== 'base64') throw refused();
    if ((wire?.extraData === 'secondsTimestamp') !== (wire?.secondsSuffix !== undefined)) throw refused();
    accountName = remoteSigner?.accountName;
    if (accountName !== undefined && (typeof accountName !== 'string' || accountName.length > 4096)) throw refused();
    for (const [slot, source] of Object.entries(options.identities)) {
      if (!source) continue;
      if (slot !== 'primary' && slot !== 'secondary') throw refused();
      const identity: RemoteSignerIdentity = { credential: source.credential, signing: source.signing,
        publicCredential: source.publicCredential, ...(source.passphrase === undefined ? {} : { passphrase: source.passphrase }) };
      if (!validRef(identity.credential) || !validRef(identity.signing)
        || identity.signing === identity.credential || owned.has(identity.signing) || owned.has(identity.credential)
        || typeof identity.publicCredential !== 'string' || !identity.publicCredential
        || /[\r\n\x00]/.test(identity.publicCredential)) throw refused();
      const bytes = Array.from(utf8(identity.publicCredential));
      const entry = { identity, primary: slot === 'primary', receiver: payloadReceiver(options.payloadSignerMetadata, identity.publicCredential) };
      owned.set(identity.signing, entry);
      owned.set(identity.credential, entry);
      credentials.set(identity.credential, { bytes, identity });
      if (identity.passphrase !== undefined || source.passphraseValue !== undefined) {
        if (!passphraseHeader || !validRef(identity.passphrase) || owned.has(identity.passphrase)
          || typeof source.passphraseValue !== 'string' || !source.passphraseValue) throw refused();
        // Reuse the shared callback-header grammar/value/total bounds. The
        // resulting bytes stay in this provider's separate Present-only map.
        headers({ [passphraseHeader]: source.passphraseValue });
        const material = Array.from(utf8(source.passphraseValue));
        const previous = passphrases.get(identity.passphrase);
        if (previous && (previous.length !== material.length || previous.some((byte, index) => byte !== material[index]))) throw refused();
        passphrases.set(identity.passphrase, material);
      }
    }
    for (const key of passphrases.keys()) if (owned.has(key)) throw refused();
    if (!remoteSigner && !payloadSigner) throw refused();
  } catch { throw refused(); }

  /** The one admitted fused shape, in the engine's order: `sha256` over bytes and
   * the signing identity's own credential, `sign sha256SecretSuffix` over exactly
   * `result{0, hex}`, then `present` of that credential. Anything else refuses.
   * The browser computes both hashes from the inner preimage; the inner digest
   * is recomputed here, and the key comes from local custody, never the browser.
   */
  const signDoubleSha256 = async (operations: SignRequest['operations'], receipt: SignReceipt, signal: AbortSignal): Promise<RemoteSignerResult> => {
    if (!doubleSha256(exchangeName) || !remoteSigner || operations.length !== 3) throw refused();
    const [digest, sign, present] = operations;
    if (digest?.kind !== 'sha256' || sign?.kind !== 'sign' || present?.kind !== 'present'
      || sign.algorithm !== 'sha256SecretSuffix') throw refused();
    const entry = owned.get(sign.key);
    if (!entry || !entry.primary || entry.identity.signing !== sign.key
      || present.key !== entry.identity.credential) throw refused();
    const reference = Array.isArray(sign.payload) && sign.payload.length === 1 ? Array.from(sign.payload)[0] : undefined;
    if (!record(reference) || reference.kind !== 'result' || reference.index !== 0 || reference.encoding !== 'hex') throw refused();
    if (!Array.isArray(digest.payload) || digest.payload.length < 1 || digest.payload.length > MAX_PARTS) throw refused();
    const material = credentials.get(entry.identity.credential);
    if (!material) throw refused();
    const key = material.bytes;
    let total = key.length;
    const parts = Array.from(digest.payload, (part) => {
      let bytes: number[];
      if (part?.kind === 'credential' && part.key === entry.identity.credential) bytes = key;
      else if (part?.kind === 'bytes' && Array.isArray(part.bytes) && part.bytes.length <= MAX_PAYLOAD) {
        for (const byte of part.bytes) {
          if (!Number.isInteger(byte) || byte < 0 || byte > 255) throw refused();
        }
        bytes = part.bytes;
      } else throw refused();
      total += bytes.length;
      if (total > MAX_PAYLOAD) throw refused();
      return bytes;
    });
    const preimage = new Uint8Array(parts.reduce((size, part) => size + part.length, 0));
    let offset = 0;
    for (const part of parts) { preimage.set(part, offset); offset += part.length; }
    const toSign = decodeUtf8(preimage);
    if (signal.aborted) throw canceled();
    const id = nanoid();
    const response = await guarded(signal, remoteSigner, () => remoteSigner.sign({
      id,
      type: 'ws-auth',
      exchangeName,
      ...(accountName === undefined ? {} : { accountName }),
      wsAuth: { toSign, outputFormat: 'bitunix-double-sha256' },
    }));
    if (!record(response) || response.id !== id
      || (response.error !== undefined && (typeof response.error !== 'string' || response.error !== ''))) throw refused();
    // Exactly 64 hex characters: browser text never reaches a header unparsed.
    const outer = mac(response.wsAuthSignature, 'hex');
    if (signal.aborted) throw canceled();
    return { values: [Array.from(sha256(preimage)), outer, key.slice()], receipt };
  };

  /** Hyperliquid never sends a bare digest. Startup binds the agent address via
   * `[publicAddress]`; each action is `[sha256(statement), publicAddress, sign
   * secp256k1Digest]`, and the device re-derives the digest from the statement.
   * The engine refuses any signature that does not recover to the bound address.
   */
  let boundAddress: number[] | undefined;
  const hexBytes = (value: unknown, length: number): number[] => {
    if (typeof value !== 'string' || !new RegExp(`^0x[0-9a-fA-F]{${length * 2}}$`).test(value)) throw refused();
    return Array.from({ length }, (_, i) => Number.parseInt(value.slice(2 + i * 2, 4 + i * 2), 16));
  };
  const askDevice = async (toSign: string, outputFormat: 'hyperliquid-l1' | 'hyperliquid-address', signal: AbortSignal) => {
    const id = nanoid();
    const response = await guarded(signal, remoteSigner, () => remoteSigner!.sign({
      id,
      type: 'ws-auth',
      exchangeName,
      ...(accountName === undefined ? {} : { accountName }),
      wsAuth: { toSign, outputFormat },
    }));
    if (!record(response) || response.id !== id
      || (response.error !== undefined && (typeof response.error !== 'string' || response.error !== ''))) throw refused();
    if (signal.aborted) throw canceled();
    return response.wsAuthSignature;
  };
  const signWallet = async (operations: SignRequest['operations'], receipt: SignReceipt, signal: AbortSignal): Promise<RemoteSignerResult> => {
    if (!remoteSigner) throw refused();
    const kinds = operations.map((operation) => (operation as { kind?: unknown })?.kind);
    const keyOf = (operation: unknown) => (record(operation) ? operation.key : undefined);
    const entry = owned.get(keyOf(operations.at(-1)) as string);
    if (!entry || !entry.primary || operations.some((operation) => record(operation)
      && 'key' in operation && operation.key !== entry.identity.signing)) throw refused();
    if (kinds.length === 1 && kinds[0] === 'publicAddress') {
      const address = hexBytes(await askDevice(entry.identity.publicCredential, 'hyperliquid-address', signal), 20);
      // One provider binds one agent; a different device key is never adopted.
      if (boundAddress && boundAddress.some((byte, index) => byte !== address[index])) throw refused();
      boundAddress = address;
      return { values: [address.slice()], receipt };
    }
    const [statement, , sign] = operations;
    if (kinds.join() !== 'sha256,publicAddress,sign' || !boundAddress || sign.kind !== 'sign'
      || sign.algorithm !== 'secp256k1Digest' || statement.kind !== 'sha256') throw refused();
    const bytes = (parts: unknown, max: number) => {
      const part = Array.isArray(parts) && parts.length === 1 ? parts[0] : undefined;
      if (!record(part) || part.kind !== 'bytes' || !Array.isArray(part.bytes) || part.bytes.length > max
        || part.bytes.some((byte) => !Number.isInteger(byte) || byte < 0 || byte > 255)) throw refused();
      return Uint8Array.from(part.bytes as number[]);
    };
    const preimage = bytes(statement.payload, MAX_PAYLOAD);
    if (bytes(sign.payload, 32).length !== 32) throw refused();
    const signature = hexBytes(await askDevice(decodeUtf8(preimage), 'hyperliquid-l1', signal), 65);
    return { values: [Array.from(sha256(preimage)), boundAddress.slice(), signature], receipt };
  };

  /** Lighter never sends bare words: `[sha256(statement), sign schnorrDigest x n]`,
   * one device round trip per request. The device re-derives every signed word
   * from the statement and answers the n signatures concatenated in order.
   */
  const signLighter = async (operations: SignRequest['operations'], receipt: SignReceipt, signal: AbortSignal): Promise<RemoteSignerResult> => {
    if (!remoteSigner) throw refused();
    const [statement, ...signs] = operations;
    if (statement?.kind !== 'sha256' || signs.length < 1) throw refused();
    const parts = Array.isArray(statement.payload) && statement.payload.length === 1 ? statement.payload[0] : undefined;
    if (!record(parts) || parts.kind !== 'bytes' || !Array.isArray(parts.bytes) || parts.bytes.length > MAX_PAYLOAD
      || parts.bytes.some((byte) => !Number.isInteger(byte) || byte < 0 || byte > 255)) throw refused();
    const preimage = Uint8Array.from(parts.bytes as number[]);
    const entry = owned.get((signs[0] as { key?: string }).key as string);
    if (!entry || !entry.primary || signs.some((sign) => sign?.kind !== 'sign' || sign.algorithm !== 'schnorrDigest'
      || sign.key !== entry.identity.signing)) throw refused();
    const id = nanoid();
    const response = await guarded(signal, remoteSigner, () => remoteSigner.sign({
      id,
      type: 'ws-auth',
      exchangeName,
      ...(accountName === undefined ? {} : { accountName }),
      wsAuth: { toSign: decodeUtf8(preimage), outputFormat: 'lighter-tx' },
    }));
    if (!record(response) || response.id !== id
      || (response.error !== undefined && (typeof response.error !== 'string' || response.error !== ''))) throw refused();
    if (signal.aborted) throw canceled();
    const length = signs.length * LIGHTER_SIGNATURE;
    const answer = response.wsAuthSignature;
    if (typeof answer !== 'string' || !new RegExp(`^0x[0-9a-fA-F]{${length * 2}}$`).test(answer)) throw refused();
    const bytes = Array.from({ length }, (_, i) => Number.parseInt(answer.slice(2 + i * 2, 4 + i * 2), 16));
    const values = [Array.from(sha256(preimage))];
    for (let i = 0; i < signs.length; i++) values.push(bytes.slice(i * LIGHTER_SIGNATURE, (i + 1) * LIGHTER_SIGNATURE));
    return { values, receipt };
  };

  return async (request, { signal }): Promise<RemoteSignerResult> => {
    try {
      if (signal.aborted) throw canceled();
      const receipt: SignReceipt = { ...request.receipt };
      if (!record(request.receipt) || Object.entries(receipt).some(([key, value]) =>
        !['stampMs', 'expiryMs'].includes(key) || (value !== undefined && (typeof value !== 'number' || !Number.isSafeInteger(value) || value < 0)))) throw refused();
      if (receipt.stampMs !== undefined && receipt.expiryMs !== undefined && receipt.expiryMs <= receipt.stampMs) throw refused();
      if (!Array.isArray(request.operations) || request.operations.length < 1 || request.operations.length > MAX_OPERATIONS) throw refused();
      const operations = Array.from(request.operations);
      if (hyperliquid(exchangeName)) return await signWallet(operations, receipt, signal);
      if (lighter(exchangeName)) return await signLighter(operations, receipt, signal);
      if (operations.some((operation) => operation?.kind === 'sha256'
        || (operation?.kind === 'sign' && operation.algorithm === 'sha256SecretSuffix'))) {
        return await signDoubleSha256(operations, receipt, signal);
      }
      let total = 0;
      // Validate the complete batch before invoking any callback.
      const work = Array.from(request.operations, (operation) => {
        if (operation.kind === 'present') {
          const item = credentials.get(operation.key);
          const material = item?.bytes ?? passphrases.get(operation.key);
          if (!material) throw refused();
          total += material.length;
          if (total > MAX_PAYLOAD) throw refused();
          return { kind: 'present' as const, key: operation.key, passphrase: item === undefined, bytes: material.slice() };
        }
        if (operation.kind !== 'sign' || !HMAC_ALGORITHMS.includes(operation.algorithm)
          || !algorithms.includes(operation.algorithm)) throw refused();
        const entry = owned.get(operation.key);
        if (!entry || entry.identity.signing !== operation.key || (remoteSigner && !entry.primary)
          || !Array.isArray(operation.payload) || operation.payload.length > MAX_PARTS) throw refused();
        const parts = Array.from(operation.payload, (part) => {
          let bytes: number[];
          if (part.kind === 'credential') {
            const item = credentials.get(part.key);
            if (!item || item.identity !== entry.identity) throw refused();
            bytes = item.bytes;
          } else if (part.kind === 'bytes' && Array.isArray(part.bytes) && part.bytes.length <= MAX_PAYLOAD) {
            for (const byte of part.bytes) {
              if (!Number.isInteger(byte) || byte < 0 || byte > 255) throw refused();
            }
            bytes = part.bytes;
          } else throw refused();
          total += bytes.length;
          if (total > MAX_PAYLOAD) throw refused();
          return bytes;
        });
        const payload = new Uint8Array(parts.reduce((size, part) => size + part.length, 0));
        let offset = 0;
        for (const part of parts) { payload.set(part, offset); offset += part.length; }
        const text = decodeUtf8(payload);
        const credentialDelimited = operation.payload.length === 3 && operation.payload[1].kind === 'credential';
        const asciiPart = (part: number[], expected: string) => part.length === expected.length
          && part.every((byte, index) => byte === expected.charCodeAt(index));
        const suffix = wire?.extraData === 'secondsTimestamp' ? wire.secondsSuffix : undefined;
        const suffixDelimited = suffix !== undefined && operation.payload.length === 3
          && operation.payload.every((part) => part.kind === 'bytes')
          && asciiPart(parts[1], suffix.method) && asciiPart(parts[2], suffix.path);
        const extraData = remoteSigner ? wsMetadata(wire, text, entry.identity.publicCredential, receipt, credentialDelimited, suffixDelimited) : undefined;
        return { kind: 'sign' as const, entry, payload: text, extraData };
      });
      for (const item of work) {
        if (item.kind === 'present' && item.passphrase
          && !work.some((candidate) => candidate.kind === 'sign' && candidate.entry.identity.passphrase === item.key)) throw refused();
      }
      const signatures = work.filter((item) => item.kind === 'sign').length;
      const values: number[][] = [];
      let metadata: [string, string][] | undefined;
      for (const item of work) {
        if (signal.aborted) throw canceled();
        if (item.kind === 'present') { values.push(item.bytes); continue; }
        if (remoteSigner) {
          const { extraData } = item;
          const id = (extraData && wire?.requestId) || nanoid();
          const response = await guarded(signal, remoteSigner, () => remoteSigner.sign({
            id,
            type: 'ws-auth',
            exchangeName: extraData ? (wire?.exchangeName ?? exchangeName) : exchangeName,
            ...(accountName === undefined ? {} : { accountName }),
            wsAuth: { toSign: item.payload, outputFormat: encoding, ...(extraData === undefined ? {} : { extraData }) },
          }));
          if (!record(response) || response.id !== id
            || (response.error !== undefined && (typeof response.error !== 'string' || response.error !== ''))) throw refused();
          values.push(mac(response.wsAuthSignature, encoding));
        } else {
          const response = await guarded(signal, undefined, () => payloadSigner!.call(item.entry.receiver, {
            exchangeName, key: item.entry.identity.publicCredential, payload: item.payload,
            scheme: 'hmac-sha256', encoding,
          }));
          if (!record(response)) throw refused();
          const extra = headers(response.headers);
          if (extra !== undefined && signatures !== 1) throw refused();
          metadata = extra;
          values.push(mac(response.signature, encoding));
        }
      }
      if (signal.aborted) throw canceled();
      return { values, receipt, ...(metadata === undefined ? {} : { headers: metadata }) };
    } catch (error) {
      // Never forward provider exceptions, response errors, preimages or key refs.
      if (error instanceof Error && [canceled().message, unavailable().message].includes(error.message)) {
        throw new Error(error.message);
      }
      throw refused();
    }
  };
}
