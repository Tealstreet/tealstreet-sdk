/**
 * Inert public-frame classifier for Coinbase's Starbase WebSocket.
 *
 * The outer public envelopes are based on independently captured live frames
 * from 2026-07-15 because Coinbase's AsyncAPI publishes channel payloads but
 * omits their JSON-RPC notification envelope. This module owns no socket,
 * dispatch, subscription state, store, routing, or activation.
 */

import type { CoinbaseStarbasePublicChannel } from './coinbase.starbase-public';

import { CoinbaseStarbaseProtocolError } from './coinbase.starbase';

export const COINBASE_STARBASE_PUBLIC_FRAME_CLASSIFIER_ACTIVATION_ENABLED = false as const;

export const COINBASE_STARBASE_PUBLIC_FRAME_CAPTURE_SHA256 = Object.freeze({
  heartbeat: '7a63af8955274769a006d4dd7ab448ff7f2c3a255a11871b2a95e9bb9e00e503',
  notification: 'a18e355e8c29e6a5699166e5dd5e67cc797345d6fda6f6e30676a06d91dba692',
  testRequest: '01bee32b135c20dea365212880829f7c34ad4eabe00bd1f13a8d0dcec544280a',
});

export type CoinbaseStarbasePublicFrameClassification =
  | {
      kind: 'public-subscription';
      channel: CoinbaseStarbasePublicChannel;
      /** The captured params boundary is passed intact; existing decoders consume its data property. */
      payload: { channel: CoinbaseStarbasePublicChannel; data: unknown };
    }
  | {
      kind: 'heartbeat';
      signal: 'heartbeat' | 'test_request';
    }
  | {
      kind: 'public-test-response';
      id: number;
      outcome: 'result';
      result: { version: string };
    }
  | {
      kind: 'public-test-response';
      id: number;
      outcome: 'error';
      error: { code: number; data?: unknown; message: string };
    }
  | {
      kind: 'unsupported-subscription';
      channel: string;
      reason: 'channel-not-a-desired-acknowledged-public-subscription';
      scope: 'unclassified';
    }
  | {
      kind: 'unknown';
      reason:
        | 'duplicate-json-key'
        | 'invalid-json'
        | 'invalid-public-test-response'
        | 'invalid-public-websocket-envelope'
        | 'unsupported-raw-frame';
    };

type CoinbaseStarbasePublicFrameContext = {
  acknowledgedPublicChannels: readonly CoinbaseStarbasePublicChannel[];
  desiredPublicChannels: readonly CoinbaseStarbasePublicChannel[];
  pendingPublicTestRequestIds: readonly number[];
};

const PUBLIC_INSTRUMENTS = ['BTC_USDC-PERPETUAL', 'ETH_USDC-PERPETUAL'] as const;
const BOOK_INTERVALS = ['100ms', 'agg2'] as const;
const CHART_RESOLUTIONS = ['1', '3', '5', '15', '30', '60', '120', '360', '720', '1D'] as const;

const isSupportedPublicChannel = (channel: string): channel is CoinbaseStarbasePublicChannel => {
  const parts = channel.split('.');
  if (parts.length === 3 && (parts[0] === 'ticker' || parts[0] === 'book')) {
    return (
      PUBLIC_INSTRUMENTS.includes(parts[1] as (typeof PUBLIC_INSTRUMENTS)[number]) &&
      BOOK_INTERVALS.includes(parts[2] as (typeof BOOK_INTERVALS)[number])
    );
  }
  if (parts.length === 4 && parts[0] === 'chart' && parts[1] === 'trades') {
    return (
      PUBLIC_INSTRUMENTS.includes(parts[2] as (typeof PUBLIC_INSTRUMENTS)[number]) &&
      CHART_RESOLUTIONS.includes(parts[3] as (typeof CHART_RESOLUTIONS)[number])
    );
  }
  return (
    parts.length === 4 &&
    parts[0] === 'instrument' &&
    parts[1] === 'state' &&
    parts[2] === 'future' &&
    ['BTC', 'ETH', 'USDC', 'any'].includes(parts[3])
  );
};

const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === 'object' && value !== null && !Array.isArray(value);

const hasExactKeys = (value: Record<string, unknown>, expected: readonly string[]): boolean => {
  const keys = Reflect.ownKeys(value);
  return keys.length === expected.length && keys.every((key) => typeof key === 'string' && expected.includes(key));
};

type JsonKeyScanResult = 'duplicate' | 'invalid' | 'valid';

/**
 * JSON.parse silently keeps the last duplicate object key. This small grammar
 * walk decodes every key string before comparison so escaped-equivalent keys
 * are rejected too. Values remain untouched and are parsed normally afterward.
 */
class JsonDuplicateKeyScanner {
  private duplicate = false;
  private index = 0;
  private readonly raw: string;

  constructor(raw: string) {
    this.raw = raw;
  }

  scan(): JsonKeyScanResult {
    try {
      this.scanValue(0);
      this.skipWhitespace();
      if (this.index !== this.raw.length) return 'invalid';
      return this.duplicate ? 'duplicate' : 'valid';
    } catch {
      return 'invalid';
    }
  }

  private scanValue(depth: number): void {
    if (depth > 128) throw new Error('JSON nesting limit exceeded');
    this.skipWhitespace();
    const token = this.raw[this.index];
    if (token === '{') return this.scanObject(depth + 1);
    if (token === '[') return this.scanArray(depth + 1);
    if (token === '"') {
      this.scanString();
      return;
    }
    if (token === '-' || (token >= '0' && token <= '9')) return this.scanNumber();
    if (this.consumeLiteral('true') || this.consumeLiteral('false') || this.consumeLiteral('null')) return;
    throw new Error('Invalid JSON value');
  }

  private scanObject(depth: number): void {
    this.index += 1;
    this.skipWhitespace();
    if (this.raw[this.index] === '}') {
      this.index += 1;
      return;
    }

    const keys = new Set<string>();
    while (true) {
      this.skipWhitespace();
      const key = this.scanString();
      if (keys.has(key)) this.duplicate = true;
      keys.add(key);
      this.skipWhitespace();
      if (this.raw[this.index] !== ':') throw new Error('Invalid JSON object');
      this.index += 1;
      this.scanValue(depth);
      this.skipWhitespace();
      const separator = this.raw[this.index];
      if (separator === '}') {
        this.index += 1;
        return;
      }
      if (separator !== ',') throw new Error('Invalid JSON object');
      this.index += 1;
    }
  }

  private scanArray(depth: number): void {
    this.index += 1;
    this.skipWhitespace();
    if (this.raw[this.index] === ']') {
      this.index += 1;
      return;
    }
    while (true) {
      this.scanValue(depth);
      this.skipWhitespace();
      const separator = this.raw[this.index];
      if (separator === ']') {
        this.index += 1;
        return;
      }
      if (separator !== ',') throw new Error('Invalid JSON array');
      this.index += 1;
    }
  }

  private scanString(): string {
    if (this.raw[this.index] !== '"') throw new Error('Invalid JSON string');
    const start = this.index;
    this.index += 1;
    while (this.index < this.raw.length) {
      const token = this.raw[this.index];
      if (token === '"') {
        this.index += 1;
        return JSON.parse(this.raw.slice(start, this.index)) as string;
      }
      this.index += token === '\\' ? 2 : 1;
    }
    throw new Error('Unterminated JSON string');
  }

  private scanNumber(): void {
    const number = /^-?(?:0|[1-9]\d*)(?:\.\d+)?(?:[eE][+-]?\d+)?/.exec(this.raw.slice(this.index));
    if (!number) throw new Error('Invalid JSON number');
    this.index += number[0].length;
  }

  private consumeLiteral(literal: 'false' | 'null' | 'true'): boolean {
    if (!this.raw.startsWith(literal, this.index)) return false;
    this.index += literal.length;
    return true;
  }

  private skipWhitespace(): void {
    while (
      this.raw[this.index] === ' ' ||
      this.raw[this.index] === '\n' ||
      this.raw[this.index] === '\r' ||
      this.raw[this.index] === '\t'
    ) {
      this.index += 1;
    }
  }
}

const validateContext = (context: CoinbaseStarbasePublicFrameContext): void => {
  for (const [name, channels] of [
    ['acknowledged', context.acknowledgedPublicChannels],
    ['desired', context.desiredPublicChannels],
  ] as const) {
    if (
      !Array.isArray(channels) ||
      channels.some((channel) => typeof channel !== 'string' || !isSupportedPublicChannel(channel)) ||
      new Set(channels).size !== channels.length
    ) {
      throw new CoinbaseStarbaseProtocolError(`Invalid ${name} Starbase public channels`);
    }
  }

  const requestIds = context.pendingPublicTestRequestIds;
  if (
    !Array.isArray(requestIds) ||
    requestIds.some((id) => !Number.isSafeInteger(id) || id <= 0) ||
    new Set(requestIds).size !== requestIds.length
  ) {
    throw new CoinbaseStarbaseProtocolError('Invalid pending Starbase public/test request IDs');
  }
};

const classifyNotification = (
  frame: Record<string, unknown>,
  acknowledgedChannels: ReadonlySet<CoinbaseStarbasePublicChannel>,
  desiredChannels: ReadonlySet<CoinbaseStarbasePublicChannel>,
): CoinbaseStarbasePublicFrameClassification => {
  if (!hasExactKeys(frame, ['jsonrpc', 'method', 'params']) || frame.jsonrpc !== '2.0') {
    return { kind: 'unknown', reason: 'invalid-public-websocket-envelope' };
  }
  if (!isRecord(frame.params)) {
    return { kind: 'unknown', reason: 'invalid-public-websocket-envelope' };
  }

  if (frame.method === 'heartbeat') {
    if (
      !hasExactKeys(frame.params, ['type']) ||
      (frame.params.type !== 'heartbeat' && frame.params.type !== 'test_request')
    ) {
      return { kind: 'unknown', reason: 'invalid-public-websocket-envelope' };
    }
    return { kind: 'heartbeat', signal: frame.params.type };
  }

  if (frame.method !== 'subscription' || !hasExactKeys(frame.params, ['channel', 'data'])) {
    return { kind: 'unknown', reason: 'invalid-public-websocket-envelope' };
  }
  const channel = frame.params.channel;
  if (typeof channel !== 'string' || channel.length === 0) {
    return { kind: 'unknown', reason: 'invalid-public-websocket-envelope' };
  }
  if (!isSupportedPublicChannel(channel) || !desiredChannels.has(channel) || !acknowledgedChannels.has(channel)) {
    return {
      kind: 'unsupported-subscription',
      channel,
      reason: 'channel-not-a-desired-acknowledged-public-subscription',
      scope: 'unclassified',
    };
  }

  const publicChannel = channel as CoinbaseStarbasePublicChannel;
  return {
    kind: 'public-subscription',
    channel: publicChannel,
    payload: { channel: publicChannel, data: frame.params.data },
  };
};

const parsePublicTestError = (value: unknown): { code: number; data?: unknown; message: string } | undefined => {
  if (!isRecord(value)) return undefined;
  const keys = Object.keys(value);
  if (
    !keys.every((key) => ['code', 'data', 'message'].includes(key)) ||
    !keys.includes('code') ||
    !keys.includes('message')
  ) {
    return undefined;
  }
  if (!Number.isSafeInteger(value.code) || typeof value.message !== 'string' || value.message.length === 0) {
    return undefined;
  }
  return {
    code: value.code as number,
    message: value.message,
    ...(Object.prototype.hasOwnProperty.call(value, 'data') ? { data: value.data } : {}),
  };
};

const classifyPublicTestResponse = (
  frame: Record<string, unknown>,
  pendingRequestIds: ReadonlySet<number>,
): CoinbaseStarbasePublicFrameClassification => {
  if (
    frame.jsonrpc !== '2.0' ||
    !Number.isSafeInteger(frame.id) ||
    (frame.id as number) <= 0 ||
    !pendingRequestIds.has(frame.id as number) ||
    Object.prototype.hasOwnProperty.call(frame, 'method') ||
    Object.prototype.hasOwnProperty.call(frame, 'params')
  ) {
    return { kind: 'unknown', reason: 'invalid-public-test-response' };
  }

  const hasResult = Object.prototype.hasOwnProperty.call(frame, 'result');
  const hasError = Object.prototype.hasOwnProperty.call(frame, 'error');
  if (hasResult === hasError) return { kind: 'unknown', reason: 'invalid-public-test-response' };

  const allowedKeys = new Set(['jsonrpc', 'id', hasResult ? 'result' : 'error', 'testnet', 'usDiff', 'usIn', 'usOut']);
  if (Reflect.ownKeys(frame).some((key) => typeof key !== 'string' || !allowedKeys.has(key))) {
    return { kind: 'unknown', reason: 'invalid-public-test-response' };
  }
  if (
    (Object.prototype.hasOwnProperty.call(frame, 'testnet') && typeof frame.testnet !== 'boolean') ||
    (Object.prototype.hasOwnProperty.call(frame, 'usIn') &&
      (!Number.isSafeInteger(frame.usIn) || (frame.usIn as number) <= 0)) ||
    (Object.prototype.hasOwnProperty.call(frame, 'usOut') &&
      (!Number.isSafeInteger(frame.usOut) || (frame.usOut as number) <= 0)) ||
    (Object.prototype.hasOwnProperty.call(frame, 'usDiff') &&
      (!Number.isSafeInteger(frame.usDiff) || (frame.usDiff as number) < 0))
  ) {
    return { kind: 'unknown', reason: 'invalid-public-test-response' };
  }
  if (
    Number.isSafeInteger(frame.usIn) &&
    Number.isSafeInteger(frame.usOut) &&
    (frame.usOut as number) < (frame.usIn as number)
  ) {
    return { kind: 'unknown', reason: 'invalid-public-test-response' };
  }
  if (
    Number.isSafeInteger(frame.usIn) &&
    Number.isSafeInteger(frame.usOut) &&
    Number.isSafeInteger(frame.usDiff) &&
    (frame.usOut as number) - (frame.usIn as number) !== frame.usDiff
  ) {
    return { kind: 'unknown', reason: 'invalid-public-test-response' };
  }

  const id = frame.id as number;
  if (hasResult) {
    if (!isRecord(frame.result) || !hasExactKeys(frame.result, ['version'])) {
      return { kind: 'unknown', reason: 'invalid-public-test-response' };
    }
    if (typeof frame.result.version !== 'string' || frame.result.version.length === 0) {
      return { kind: 'unknown', reason: 'invalid-public-test-response' };
    }
    return { kind: 'public-test-response', id, outcome: 'result', result: { version: frame.result.version } };
  }

  const error = parsePublicTestError(frame.error);
  if (!error) return { kind: 'unknown', reason: 'invalid-public-test-response' };
  return { kind: 'public-test-response', id, outcome: 'error', error };
};

export const classifyCoinbaseStarbasePublicWebsocketFrame = (
  rawFrame: unknown,
  context: CoinbaseStarbasePublicFrameContext,
): CoinbaseStarbasePublicFrameClassification => {
  validateContext(context);
  if (typeof rawFrame !== 'string') return { kind: 'unknown', reason: 'unsupported-raw-frame' };

  const keyScan = new JsonDuplicateKeyScanner(rawFrame).scan();
  if (keyScan === 'duplicate') return { kind: 'unknown', reason: 'duplicate-json-key' };
  if (keyScan === 'invalid') return { kind: 'unknown', reason: 'invalid-json' };

  let parsed: unknown;
  try {
    parsed = JSON.parse(rawFrame) as unknown;
  } catch {
    return { kind: 'unknown', reason: 'invalid-json' };
  }
  if (!isRecord(parsed)) return { kind: 'unknown', reason: 'invalid-public-websocket-envelope' };

  if (Object.prototype.hasOwnProperty.call(parsed, 'method')) {
    return classifyNotification(
      parsed,
      new Set(context.acknowledgedPublicChannels),
      new Set(context.desiredPublicChannels),
    );
  }
  if (Object.prototype.hasOwnProperty.call(parsed, 'id')) {
    return classifyPublicTestResponse(parsed, new Set(context.pendingPublicTestRequestIds));
  }
  return { kind: 'unknown', reason: 'invalid-public-websocket-envelope' };
};
