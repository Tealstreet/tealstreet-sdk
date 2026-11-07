import type { WooV3Decimal } from './woo.v3.contracts';
import type { WooV3RequestInput } from './woo.v3.request';

import { WOO_V3_ENDPOINTS, WOO_V3_WS_URL } from './woo.v3.contracts';

/**
 * Inert, transport-free contracts for the namespaced WOO V3 WebSocket API.
 *
 * This file owns no socket, credentials, timers, mutable store, or reconnect
 * loop. The live Woo exchange and its legacy WebSocket classes deliberately do
 * not import it. It decodes only the shapes published in WOO's current V3
 * documentation and exposes blockers wherever those pages do not define
 * enough lifecycle semantics for a store-authoritative adapter.
 */

type UnknownRecord = Record<string, unknown>;

export const WOO_V3_WS_CHANNELS = Object.freeze({
  public: Object.freeze({
    livenetUrl: WOO_V3_WS_URL.public.livenet,
    testnetUrl: WOO_V3_WS_URL.public.testnet,
    authentication: 'none' as const,
  }),
  private: Object.freeze({
    livenetUrl: WOO_V3_WS_URL.private.livenet,
    testnetUrl: WOO_V3_WS_URL.private.testnet,
    authentication: 'listen-key-query' as const,
    listenKeyEndpoint: WOO_V3_ENDPOINTS.ACCOUNT_LISTEN_KEY,
  }),
});

export const WOO_V3_WS_LIMITS = Object.freeze({
  maximumConnectionLifetimeMs: 24 * 60 * 60 * 1_000,
  maximumConnectionsPerFiveMinutesPerIp: 100,
  maximumActiveConnectionsPerIp: 100,
  maximumTopicsPerConnection: 100,
  maximumTopicsPerCommand: 20,
  recommendedMaximumSubscriptionCommandsPerSecond: 200,
});

export const WOO_V3_WS_PUBLIC_TOPIC_KINDS = Object.freeze([
  'orderbookupdate',
  'orderbookupdaterpi',
  'orderbook10',
  'bbo',
  'bborpi',
  'trade',
  'ticker',
  'kline',
  'indexprice',
  'markprice',
  'estfundingrate',
  'openinterest',
] as const);

export const WOO_V3_WS_PRIVATE_TOPICS = Object.freeze([
  'account',
  'balance',
  'position',
  'executionreport',
  'algoexecutionreport',
] as const);

export type WooV3WsPublicTopicKind = (typeof WOO_V3_WS_PUBLIC_TOPIC_KINDS)[number];
export type WooV3WsPrivateTopic = (typeof WOO_V3_WS_PRIVATE_TOPICS)[number];
export type WooV3WsChannel = 'public' | 'private';
export type WooV3WsBookDepth = 50 | 200 | 500;
export type WooV3WsKlineInterval =
  | '1m'
  | '3m'
  | '5m'
  | '15m'
  | '30m'
  | '1h'
  | '2h'
  | '4h'
  | '6h'
  | '12h'
  | '1d'
  | '3d'
  | '1w'
  | '1M';

export type WooV3WsTopicDescriptor =
  | Readonly<{
      channel: 'public';
      kind: 'orderbookupdate' | 'orderbookupdaterpi';
      symbol: string;
      depth: WooV3WsBookDepth;
      includesRpi: boolean;
      topic: string;
    }>
  | Readonly<{
      channel: 'public';
      kind: 'kline';
      symbol: string;
      interval: WooV3WsKlineInterval;
      topic: string;
    }>
  | Readonly<{
      channel: 'public';
      kind: Exclude<WooV3WsPublicTopicKind, 'orderbookupdate' | 'orderbookupdaterpi' | 'kline'>;
      symbol: string;
      topic: string;
    }>
  | Readonly<{
      channel: 'private';
      kind: WooV3WsPrivateTopic;
      topic: WooV3WsPrivateTopic;
    }>;

export type WooV3WsBlockerCode =
  | 'command-failure-envelope-undocumented'
  | 'command-request-id-type-undocumented'
  | 'listen-key-rotation-overlap-undocumented'
  | 'private-account-replacement-semantics-undocumented'
  | 'private-balance-replacement-semantics-undocumented'
  | 'private-position-replacement-semantics-undocumented'
  | 'execution-status-lifecycle-incomplete'
  | 'algo-shape-and-lifecycle-incomplete';

export type WooV3WsBlocker = Readonly<{
  code: WooV3WsBlockerCode;
  reason: string;
  requires: 'official-contract' | 'credentialed-demo';
}>;

export const WOO_V3_WS_BLOCKERS: Readonly<Record<WooV3WsBlockerCode, WooV3WsBlocker>> = Object.freeze({
  'command-failure-envelope-undocumented': Object.freeze({
    code: 'command-failure-envelope-undocumented',
    reason: 'WOO documents only success=true command acknowledgements, not the complete failure envelope',
    requires: 'official-contract',
  }),
  'command-request-id-type-undocumented': Object.freeze({
    code: 'command-request-id-type-undocumented',
    reason:
      'WOO shows an untyped request id placeholder and a string response id; builders use the documented string response subset without claiming numeric ids are invalid',
    requires: 'official-contract',
  }),
  'listen-key-rotation-overlap-undocumented': Object.freeze({
    code: 'listen-key-rotation-overlap-undocumented',
    reason: 'creating a new listen key immediately expires the old key, but reconnect/rotation overlap is not defined',
    requires: 'credentialed-demo',
  }),
  'private-account-replacement-semantics-undocumented': Object.freeze({
    code: 'private-account-replacement-semantics-undocumented',
    reason: 'account pushes are periodic and event-driven, but WOO does not define snapshot versus patch replacement semantics',
    requires: 'credentialed-demo',
  }),
  'private-balance-replacement-semantics-undocumented': Object.freeze({
    code: 'private-balance-replacement-semantics-undocumented',
    reason: 'balance pushes contain an array, but WOO does not state whether omitted tokens are unchanged or removed',
    requires: 'credentialed-demo',
  }),
  'private-position-replacement-semantics-undocumented': Object.freeze({
    code: 'private-position-replacement-semantics-undocumented',
    reason: 'position pushes contain an array, but WOO does not state whether omitted positions are unchanged or closed',
    requires: 'credentialed-demo',
  }),
  'execution-status-lifecycle-incomplete': Object.freeze({
    code: 'execution-status-lifecycle-incomplete',
    reason: 'the execution page shows selected status transitions and reject variants but no exhaustive status lifecycle',
    requires: 'credentialed-demo',
  }),
  'algo-shape-and-lifecycle-incomplete': Object.freeze({
    code: 'algo-shape-and-lifecycle-incomplete',
    reason: 'the algo page shows one BRACKET row without an exhaustive per-algo optional-field or lifecycle contract',
    requires: 'credentialed-demo',
  }),
});

const WS_SYMBOL_PATTERN = /^(?:SPOT|PERP)_[A-Z0-9]+_[A-Z0-9]+$/;
const DECIMAL_PATTERN = /^-?(?:\d+(?:\.\d*)?|\.\d+)$/;
const KLINE_INTERVALS: readonly WooV3WsKlineInterval[] = [
  '1m',
  '3m',
  '5m',
  '15m',
  '30m',
  '1h',
  '2h',
  '4h',
  '6h',
  '12h',
  '1d',
  '3d',
  '1w',
  '1M',
];

const isRecord = (value: unknown): value is UnknownRecord =>
  value !== null && typeof value === 'object' && !Array.isArray(value);

const requireRecord = (value: unknown, path: string): UnknownRecord => {
  if (!isRecord(value)) throw new Error(`${path} must be an object`);
  return value;
};

const requireArray = (value: unknown, path: string): readonly unknown[] => {
  if (!Array.isArray(value)) throw new Error(`${path} must be an array`);
  return value;
};

const requireString = (value: unknown, path: string, allowEmpty = false): string => {
  if (typeof value !== 'string' || (!allowEmpty && value.length === 0)) {
    throw new Error(`${path} must be ${allowEmpty ? 'a string' : 'a non-empty string'}`);
  }
  return value;
};

const requireBoolean = (value: unknown, path: string): boolean => {
  if (typeof value !== 'boolean') throw new Error(`${path} must be a boolean`);
  return value;
};

const requireInteger = (value: unknown, path: string, minimum = 0): number => {
  if (!Number.isSafeInteger(value) || (value as number) < minimum) {
    throw new Error(`${path} must be a safe integer of at least ${minimum}`);
  }
  return value as number;
};

const requireDecimal = (value: unknown, path: string): WooV3Decimal => {
  if (typeof value !== 'string' || !DECIMAL_PATTERN.test(value)) {
    throw new Error(`${path} must be a finite decimal string`);
  }
  return value;
};

const requireNonNegativeDecimal = (value: unknown, path: string): WooV3Decimal => {
  const decimal = requireDecimal(value, path);
  if (decimal.startsWith('-')) throw new Error(`${path} must be a non-negative decimal string`);
  return decimal;
};

const requirePositiveDecimal = (value: unknown, path: string): WooV3Decimal => {
  const decimal = requireDecimal(value, path);
  if (decimal.startsWith('-') || !/[1-9]/.test(decimal)) {
    throw new Error(`${path} must be a positive decimal string`);
  }
  return decimal;
};

const requireEnum = <T extends string>(value: unknown, values: readonly T[], path: string): T => {
  if (typeof value !== 'string' || !values.includes(value as T)) {
    throw new Error(`${path} must be one of ${values.join(', ')}`);
  }
  return value as T;
};

const requireSymbol = (value: unknown, path: string): string => {
  const symbol = requireString(value, path);
  if (!WS_SYMBOL_PATTERN.test(symbol)) throw new Error(`${path} must be a documented WOO SPOT_ or PERP_ symbol`);
  return symbol;
};

const requireTopicList = (value: unknown, path: string): readonly string[] => {
  const topics = requireArray(value, path).map((topic, index) => requireString(topic, `${path}[${index}]`));
  if (new Set(topics).size !== topics.length) throw new Error(`${path} must not contain duplicate topics`);
  return Object.freeze(topics);
};

export const buildWooV3ListenKeyRequest = (): WooV3RequestInput =>
  Object.freeze({
    method: 'POST',
    path: WOO_V3_ENDPOINTS.ACCOUNT_LISTEN_KEY,
    body: Object.freeze({ type: 'WEBSOCKET' }),
  });

export type WooV3ListenKey = Readonly<{ authKey: string; expiredTime: number; responseTimestamp: number }>;

export const decodeWooV3ListenKey = (input: unknown): WooV3ListenKey => {
  const response = requireRecord(input, 'WOO V3 listen key response');
  if (requireBoolean(response.success, 'WOO V3 listen key.success') !== true) {
    throw new Error('WOO V3 listen key response reports success=false');
  }
  const responseTimestamp = requireInteger(response.timestamp, 'WOO V3 listen key.timestamp');
  const data = requireRecord(response.data, 'WOO V3 listen key.data');
  const expiredTime = requireInteger(data.expiredTime, 'WOO V3 listen key.data.expiredTime', 1);
  if (expiredTime <= responseTimestamp) throw new Error('WOO V3 listen key must expire after its response timestamp');
  return Object.freeze({
    authKey: requireString(data.authKey, 'WOO V3 listen key.data.authKey'),
    expiredTime,
    responseTimestamp,
  });
};

export const buildWooV3PrivateWebSocketUrl = (
  environment: 'livenet' | 'testnet',
  authKey: string,
): string => {
  const key = requireString(authKey, 'WOO V3 private websocket authKey');
  const base = environment === 'livenet' ? WOO_V3_WS_URL.private.livenet : WOO_V3_WS_URL.private.testnet;
  return `${base}?key=${encodeURIComponent(key)}`;
};

export const buildWooV3PublicTopic = (
  kind: WooV3WsPublicTopicKind,
  symbolInput: string,
  option?: WooV3WsBookDepth | WooV3WsKlineInterval,
): WooV3WsTopicDescriptor => {
  const symbol = requireSymbol(symbolInput, 'WOO V3 websocket topic symbol');
  if (kind === 'orderbookupdate' || kind === 'orderbookupdaterpi') {
    if (option !== 50 && option !== 200 && option !== 500) {
      throw new Error(`${kind} requires documented depth 50, 200, or 500`);
    }
    return Object.freeze({
      channel: 'public',
      kind,
      symbol,
      depth: option,
      includesRpi: kind === 'orderbookupdaterpi',
      topic: `${kind}@${symbol}@${option}`,
    });
  }
  if (kind === 'kline') {
    if (typeof option !== 'string' || !KLINE_INTERVALS.includes(option as WooV3WsKlineInterval)) {
      throw new Error('kline requires a documented WOO V3 interval');
    }
    return Object.freeze({ channel: 'public', kind, symbol, interval: option as WooV3WsKlineInterval, topic: `${kind}@${symbol}@${option}` });
  }
  if (option !== undefined) throw new Error(`${kind} does not accept a topic option`);
  return Object.freeze({ channel: 'public', kind, symbol, topic: `${kind}@${symbol}` });
};

export const buildWooV3PrivateTopic = (topic: WooV3WsPrivateTopic): WooV3WsTopicDescriptor => {
  if (!WOO_V3_WS_PRIVATE_TOPICS.includes(topic)) throw new Error(`unknown WOO V3 private topic ${String(topic)}`);
  return Object.freeze({ channel: 'private', kind: topic, topic });
};

export const parseWooV3WsTopic = (input: unknown): WooV3WsTopicDescriptor => {
  const topic = requireString(input, 'WOO V3 websocket topic');
  if (WOO_V3_WS_PRIVATE_TOPICS.includes(topic as WooV3WsPrivateTopic)) {
    return buildWooV3PrivateTopic(topic as WooV3WsPrivateTopic);
  }
  const parts = topic.split('@');
  const kind = parts[0] as WooV3WsPublicTopicKind;
  if (!WOO_V3_WS_PUBLIC_TOPIC_KINDS.includes(kind)) throw new Error(`unknown WOO V3 websocket topic ${topic}`);
  if (kind === 'orderbookupdate' || kind === 'orderbookupdaterpi') {
    if (parts.length !== 3) throw new Error(`${kind} topic must contain symbol and depth`);
    const depth = Number(parts[2]);
    return buildWooV3PublicTopic(kind, parts[1]!, depth as WooV3WsBookDepth);
  }
  if (kind === 'kline') {
    if (parts.length !== 3) throw new Error('kline topic must contain symbol and interval');
    return buildWooV3PublicTopic(kind, parts[1]!, parts[2] as WooV3WsKlineInterval);
  }
  if (parts.length !== 2) throw new Error(`${kind} topic must contain exactly one symbol`);
  return buildWooV3PublicTopic(kind, parts[1]!);
};

export type WooV3WsCommandName = 'SUBSCRIBE' | 'UN_SUBSCRIBE' | 'UN_SUBSCRIBE_ALL' | 'LIST_SUBSCRIPTION';
export type WooV3WsWireCommand = Readonly<{
  id: string;
  cmd: WooV3WsCommandName;
  params?: readonly string[];
}>;

/** Channel is transport metadata; `wire` is the exact JSON command payload. */
export type WooV3WsCommand = Readonly<{
  channel: WooV3WsChannel;
  wire: WooV3WsWireCommand;
}>;

export const WOO_V3_WS_COMMAND_ID_CONTRACT_STATUS = Object.freeze({
  emittedSubset: 'documented-response-string' as const,
  blocker: WOO_V3_WS_BLOCKERS['command-request-id-type-undocumented'],
});

const assertTopicsMatchChannel = (
  topics: readonly string[],
  channel: WooV3WsChannel,
  path: string,
): void => {
  topics.forEach((topic, index) => {
    const descriptor = parseWooV3WsTopic(topic);
    if (descriptor.channel !== channel) {
      throw new Error(`${path}[${index}] is a ${descriptor.channel} topic for a ${channel} websocket`);
    }
  });
};

const buildCommand = (
  channel: WooV3WsChannel,
  idInput: string,
  cmd: WooV3WsCommandName,
  topics?: readonly string[],
): WooV3WsCommand => {
  const id = requireString(idInput, 'WOO V3 websocket command id');
  if (topics !== undefined) {
    if (topics.length < 1 || topics.length > WOO_V3_WS_LIMITS.maximumTopicsPerCommand) {
      throw new Error(`WOO V3 ${cmd} requires 1-${WOO_V3_WS_LIMITS.maximumTopicsPerCommand} topics`);
    }
    assertTopicsMatchChannel(topics, channel, `WOO V3 ${cmd} topics`);
    if (new Set(topics).size !== topics.length) throw new Error(`WOO V3 ${cmd} topics must be unique`);
    return Object.freeze({
      channel,
      wire: Object.freeze({ id, cmd, params: Object.freeze([...topics]) }),
    });
  }
  return Object.freeze({ channel, wire: Object.freeze({ id, cmd }) });
};

export const buildWooV3Subscribe = (
  channel: WooV3WsChannel,
  id: string,
  topics: readonly string[],
): WooV3WsCommand => buildCommand(channel, id, 'SUBSCRIBE', topics);

export const buildWooV3Unsubscribe = (
  channel: WooV3WsChannel,
  id: string,
  topics: readonly string[],
): WooV3WsCommand => buildCommand(channel, id, 'UN_SUBSCRIBE', topics);

export const buildWooV3UnsubscribeAll = (channel: WooV3WsChannel, id: string): WooV3WsCommand =>
  buildCommand(channel, id, 'UN_SUBSCRIBE_ALL');

export const buildWooV3ListSubscriptions = (channel: WooV3WsChannel, id: string): WooV3WsCommand =>
  buildCommand(channel, id, 'LIST_SUBSCRIPTION');

export type WooV3WsCommandAck = Readonly<{
  id: string;
  cmd: WooV3WsCommandName;
  success: true;
  time: number;
  data: readonly string[];
}>;

export const decodeWooV3WsCommandAck = (input: unknown, expected?: WooV3WsCommand): WooV3WsCommandAck => {
  const frame = requireRecord(input, 'WOO V3 websocket command acknowledgement');
  const id = requireString(frame.id, 'WOO V3 websocket command acknowledgement.id');
  const cmd = requireEnum(
    frame.cmd,
    ['SUBSCRIBE', 'UN_SUBSCRIBE', 'UN_SUBSCRIBE_ALL', 'LIST_SUBSCRIPTION'] as const,
    'WOO V3 websocket command acknowledgement.cmd',
  );
  if (requireBoolean(frame.success, 'WOO V3 websocket command acknowledgement.success') !== true) {
    throw new Error(WOO_V3_WS_BLOCKERS['command-failure-envelope-undocumented'].reason);
  }
  const data = requireTopicList(frame.data, 'WOO V3 websocket command acknowledgement.data');
  const topicDescriptors = data.map((topic) => parseWooV3WsTopic(topic));
  const acknowledgedChannels = new Set(topicDescriptors.map((descriptor) => descriptor.channel));
  if (acknowledgedChannels.size > 1) {
    throw new Error('WOO V3 websocket command acknowledgement mixes public and private topics');
  }
  const result = Object.freeze({
    id,
    cmd,
    success: true as const,
    time: requireInteger(frame.time, 'WOO V3 websocket command acknowledgement.time'),
    data,
  });
  if (expected) {
    if (result.id !== expected.wire.id || result.cmd !== expected.wire.cmd) {
      throw new Error('WOO V3 websocket acknowledgement identity does not match its command');
    }
    const acknowledgedChannel = topicDescriptors[0]?.channel;
    if (acknowledgedChannel !== undefined && acknowledgedChannel !== expected.channel) {
      throw new Error(`WOO V3 websocket acknowledgement returned ${acknowledgedChannel} topics for a ${expected.channel} command`);
    }
    if (
      expected.wire.params &&
      (result.data.length !== expected.wire.params.length ||
        result.data.some((value, index) => value !== expected.wire.params![index]))
    ) {
      throw new Error('WOO V3 websocket acknowledgement topics do not exactly match its command');
    }
  }
  return result;
};

export type WooV3WsPing = Readonly<{ cmd: 'PING'; ts: number }>;
export type WooV3WsPong = Readonly<{ cmd: 'PONG'; success: true; time: number }>;

export const buildWooV3WsPing = (timestamp: number): WooV3WsPing =>
  Object.freeze({ cmd: 'PING', ts: requireInteger(timestamp, 'WOO V3 websocket ping timestamp') });

export const decodeWooV3WsPong = (input: unknown): WooV3WsPong => {
  const frame = requireRecord(input, 'WOO V3 websocket pong');
  if (frame.cmd !== 'PONG') throw new Error('WOO V3 websocket pong.cmd must be PONG');
  if (requireBoolean(frame.success, 'WOO V3 websocket pong.success') !== true) {
    throw new Error('WOO V3 websocket pong reports success=false');
  }
  return Object.freeze({ cmd: 'PONG', success: true, time: requireInteger(frame.time, 'WOO V3 websocket pong.time') });
};

export const WOO_V3_WS_HEARTBEAT = Object.freeze({
  classification: 'optional-latency-probe-not-keepalive' as const,
  serverInitiatedPingDocumented: false,
  clientPingCommand: 'PING' as const,
  serverPongCommand: 'PONG' as const,
});

export type WooV3WsBookLevel = readonly [WooV3Decimal, WooV3Decimal];
export type WooV3WsOrderBookUpdate = Readonly<{
  symbol: string;
  previousTimestamp: number;
  timestamp: number;
  asks: readonly WooV3WsBookLevel[];
  bids: readonly WooV3WsBookLevel[];
}>;

export type WooV3WsOrderBook10 = Readonly<{
  symbol: string;
  timestamp: number;
  asks: readonly WooV3WsBookLevel[];
  bids: readonly WooV3WsBookLevel[];
}>;

export type WooV3WsBbo = Readonly<{
  symbol: string;
  askPrice: WooV3Decimal;
  askQuantity: WooV3Decimal;
  bidPrice: WooV3Decimal;
  bidQuantity: WooV3Decimal;
  timestamp: number;
}>;

export type WooV3WsTrade = Readonly<{
  symbol: string;
  price: WooV3Decimal;
  quantity: WooV3Decimal;
  side: 'BUY' | 'SELL';
  source: 0 | 1;
  matchedRpi: boolean;
  timestamp: number;
}>;

export type WooV3WsTicker = Readonly<{
  symbol: string;
  open: WooV3Decimal;
  close: WooV3Decimal;
  high: WooV3Decimal;
  low: WooV3Decimal;
  volume: WooV3Decimal;
  amount: WooV3Decimal;
  aggregatedVolume: WooV3Decimal;
  aggregatedAmount: WooV3Decimal;
  tradeCount: number;
  timestamp: number;
  lastTradeTimestamp: number;
}>;

export type WooV3WsKline = Readonly<{
  symbol: string;
  interval: WooV3WsKlineInterval;
  open: WooV3Decimal;
  close: WooV3Decimal;
  high: WooV3Decimal;
  low: WooV3Decimal;
  volume: WooV3Decimal;
  amount: WooV3Decimal;
  startTimestamp: number;
  endTimestamp: number;
  timestamp: number;
  lastTradeTimestamp: number;
}>;

export type WooV3WsPrice = Readonly<{ symbol: string; price: WooV3Decimal; timestamp: number }>;
export type WooV3WsFundingRate = Readonly<{
  symbol: string;
  rate: WooV3Decimal;
  nextFundingTimestamp: number;
  timestamp: number;
}>;
export type WooV3WsOpenInterest = Readonly<{ symbol: string; openInterest: WooV3Decimal; timestamp: number }>;

const decodeLevels = (value: unknown, path: string, allowZeroQuantity: boolean): readonly WooV3WsBookLevel[] =>
  Object.freeze(
    requireArray(value, path).map((input, index) => {
      if (!Array.isArray(input) || input.length !== 2) throw new Error(`${path}[${index}] must be a price/quantity tuple`);
      return Object.freeze([
        requirePositiveDecimal(input[0], `${path}[${index}][0]`),
        allowZeroQuantity
          ? requireNonNegativeDecimal(input[1], `${path}[${index}][1]`)
          : requirePositiveDecimal(input[1], `${path}[${index}][1]`),
      ] as const);
    }),
  );

const assertPayloadSymbol = (row: UnknownRecord, descriptor: Extract<WooV3WsTopicDescriptor, { channel: 'public' }>, path: string): string => {
  const symbol = requireSymbol(row.s, `${path}.s`);
  if (symbol !== descriptor.symbol) throw new Error(`${path}.s does not match frame topic symbol`);
  return symbol;
};

const decodePublicPayload = (
  descriptor: Extract<WooV3WsTopicDescriptor, { channel: 'public' }>,
  input: unknown,
): WooV3WsPublicPayload => {
  const path = `WOO V3 websocket ${descriptor.kind}.data`;
  const row = requireRecord(input, path);
  const symbol = assertPayloadSymbol(row, descriptor, path);
  switch (descriptor.kind) {
    case 'orderbookupdate':
    case 'orderbookupdaterpi': {
      const previousTimestamp = requireInteger(row.prevTs, `${path}.prevTs`);
      const timestamp = requireInteger(row.ts, `${path}.ts`);
      if (timestamp <= previousTimestamp) throw new Error(`${path}.ts must follow prevTs`);
      return Object.freeze({ symbol, previousTimestamp, timestamp, asks: decodeLevels(row.asks, `${path}.asks`, true), bids: decodeLevels(row.bids, `${path}.bids`, true) });
    }
    case 'orderbook10':
      return Object.freeze({ symbol, timestamp: requireInteger(row.ts, `${path}.ts`), asks: decodeLevels(row.asks, `${path}.asks`, false), bids: decodeLevels(row.bids, `${path}.bids`, false) });
    case 'bbo':
    case 'bborpi':
      return Object.freeze({ symbol, askPrice: requirePositiveDecimal(row.ap, `${path}.ap`), askQuantity: requirePositiveDecimal(row.aq, `${path}.aq`), bidPrice: requirePositiveDecimal(row.bp, `${path}.bp`), bidQuantity: requirePositiveDecimal(row.bq, `${path}.bq`), timestamp: requireInteger(row.ts, `${path}.ts`) });
    case 'trade': {
      const source = requireInteger(row.src, `${path}.src`);
      if (source !== 0 && source !== 1) throw new Error(`${path}.src must be 0 or 1`);
      return Object.freeze({ symbol, price: requirePositiveDecimal(row.px, `${path}.px`), quantity: requirePositiveDecimal(row.sx, `${path}.sx`), side: requireEnum(row.sd, ['BUY', 'SELL'] as const, `${path}.sd`), source, matchedRpi: requireBoolean(row.rpi, `${path}.rpi`), timestamp: requireInteger(row.ts, `${path}.ts`) });
    }
    case 'ticker':
      return Object.freeze({ symbol, open: requirePositiveDecimal(row.o, `${path}.o`), close: requirePositiveDecimal(row.c, `${path}.c`), high: requirePositiveDecimal(row.h, `${path}.h`), low: requirePositiveDecimal(row.l, `${path}.l`), volume: requireNonNegativeDecimal(row.v, `${path}.v`), amount: requireNonNegativeDecimal(row.a, `${path}.a`), aggregatedVolume: requireNonNegativeDecimal(row.q, `${path}.q`), aggregatedAmount: requireNonNegativeDecimal(row.u, `${path}.u`), tradeCount: requireInteger(row.cnt, `${path}.cnt`), timestamp: requireInteger(row.ts, `${path}.ts`), lastTradeTimestamp: requireInteger(row.tts, `${path}.tts`) });
    case 'kline': {
      const interval = requireEnum(row.t, KLINE_INTERVALS, `${path}.t`);
      if (interval !== descriptor.interval) throw new Error(`${path}.t does not match frame topic interval`);
      const startTimestamp = requireInteger(row.st, `${path}.st`);
      const endTimestamp = requireInteger(row.et, `${path}.et`);
      if (endTimestamp <= startTimestamp) throw new Error(`${path}.et must follow st`);
      return Object.freeze({ symbol, interval, open: requirePositiveDecimal(row.o, `${path}.o`), close: requirePositiveDecimal(row.c, `${path}.c`), high: requirePositiveDecimal(row.h, `${path}.h`), low: requirePositiveDecimal(row.l, `${path}.l`), volume: requireNonNegativeDecimal(row.v, `${path}.v`), amount: requireNonNegativeDecimal(row.a, `${path}.a`), startTimestamp, endTimestamp, timestamp: requireInteger(row.ts, `${path}.ts`), lastTradeTimestamp: requireInteger(row.tts, `${path}.tts`) });
    }
    case 'indexprice':
    case 'markprice':
      return Object.freeze({ symbol, price: requirePositiveDecimal(row.px, `${path}.px`), timestamp: requireInteger(row.ts, `${path}.ts`) });
    case 'estfundingrate':
      return Object.freeze({ symbol, rate: requireDecimal(row.r, `${path}.r`), nextFundingTimestamp: requireInteger(row.ft, `${path}.ft`), timestamp: requireInteger(row.ts, `${path}.ts`) });
    case 'openinterest':
      return Object.freeze({ symbol, openInterest: requireNonNegativeDecimal(row.oi, `${path}.oi`), timestamp: requireInteger(row.ts, `${path}.ts`) });
  }
};

export type WooV3WsPublicPayload =
  | WooV3WsOrderBookUpdate
  | WooV3WsOrderBook10
  | WooV3WsBbo
  | WooV3WsTrade
  | WooV3WsTicker
  | WooV3WsKline
  | WooV3WsPrice
  | WooV3WsFundingRate
  | WooV3WsOpenInterest;

export type WooV3WsAccount =
  | Readonly<{ mode: 'PURE_SPOT'; totalAccountValue: WooV3Decimal }>
  | Readonly<{ mode: 'MARGIN'; leverage: number; marginRatio: WooV3Decimal; openMarginRatio: WooV3Decimal; initialMarginRatio: WooV3Decimal; maintenanceMarginRatio: WooV3Decimal; totalCollateral: WooV3Decimal; freeCollateral: WooV3Decimal; totalAccountValue: WooV3Decimal }>
  | Readonly<{ mode: 'FUTURE'; defaultMarginMode: 'CROSS' | 'ISOLATED'; positionMode: 'ONE_WAY' | 'HEDGE_MODE'; marginRatio: WooV3Decimal; openMarginRatio: WooV3Decimal; initialMarginRatio: WooV3Decimal; maintenanceMarginRatio: WooV3Decimal; totalCollateral: WooV3Decimal; freeCollateral: WooV3Decimal; totalAccountValue: WooV3Decimal }>;

export type WooV3WsBalanceRow = Readonly<{
  token: string; holding: WooV3Decimal; frozen: WooV3Decimal; interest: WooV3Decimal;
  pendingShortQuantity: WooV3Decimal; pendingLongQuantity: WooV3Decimal; staked: WooV3Decimal;
  unbonding: WooV3Decimal; vault: WooV3Decimal; launchpadVault: WooV3Decimal; earn: WooV3Decimal;
  averageOpenPrice: WooV3Decimal; pnl24h: WooV3Decimal; roi24h: WooV3Decimal; fee24h: WooV3Decimal;
  markPrice: WooV3Decimal; version: number; trialFundBonus: WooV3Decimal; lossCoverRatio: number; timestamp: number;
}>;

export type WooV3WsPositionRow = Readonly<{
  symbol: string; holding: WooV3Decimal; pendingLongQuantity: WooV3Decimal; pendingShortQuantity: WooV3Decimal;
  averageOpenPrice: WooV3Decimal; pnl24h: WooV3Decimal; roi24h: WooV3Decimal; fee24h: WooV3Decimal;
  settlePrice: WooV3Decimal; markPrice: WooV3Decimal; openingTimestamp: number; adlQuantile: number;
  leverage: number; marginMode: 'CROSS' | 'ISOLATED'; positionSide: 'BOTH' | 'LONG' | 'SHORT';
  isolatedToken: string; isolatedAmount: WooV3Decimal; isolatedFrozenLong: WooV3Decimal;
  isolatedFrozenShort: WooV3Decimal; version: number; timestamp: number;
}>;

export type WooV3WsExecutionReport =
  | Readonly<{ messageType: 0; symbol: string; clientOrderId: number; orderId: number; orderType: string; side: 'BUY' | 'SELL'; positionSide: 'BOTH' | 'LONG' | 'SHORT'; quantity: WooV3Decimal; price: WooV3Decimal; tradeId: number; executedQuantity: WooV3Decimal; executedPrice: WooV3Decimal; fee: WooV3Decimal; feeAsset: string; totalExecutedQuantity: WooV3Decimal; averageExecutedPrice: WooV3Decimal; status: string; reason: string; orderTag: string; totalFee: WooV3Decimal; feeCurrency: string; totalRebate: WooV3Decimal; rebateCurrency: string; visibleQuantity: WooV3Decimal; timestamp: number; reduceOnly: boolean; maker: boolean; leverage: number; matchedRpi: boolean; marginMode: 'CROSS' | 'ISOLATED' }>
  | Readonly<{ messageType: 1 | 2; symbol: string; orderId: number; clientOrderId: number; reason: string }>
  | Readonly<{ messageType: 3; symbol: string; reason: string }>;

export type WooV3WsAlgoExecutionRow = Readonly<{
  symbol: string; rootAlgoOrderId: number; parentAlgoOrderId: number; algoOrderId: number; clientAlgoOrderId: number;
  orderTag: string; status: string; algoType: string; side: 'BUY' | 'SELL'; positionSide: 'BOTH' | 'LONG' | 'SHORT';
  quantity: WooV3Decimal; triggerPrice: WooV3Decimal; triggerPriceType: string; triggerStatus: string;
  price: WooV3Decimal; orderType: string; triggerExecutionPrice: WooV3Decimal; triggerTimestamp: number;
  tradeId: number; executedPrice: WooV3Decimal; executedQuantity: WooV3Decimal; fee: WooV3Decimal; feeAsset: string;
  reason: string; totalExecutedQuantity: WooV3Decimal; averageExecutedPrice: WooV3Decimal; totalFee: WooV3Decimal;
  feeCurrency: string; totalRebate: WooV3Decimal; rebateCurrency: string; visibleQuantity: WooV3Decimal;
  timestamp: number; reduceOnly: boolean; activatedPrice: WooV3Decimal; callbackRatio: WooV3Decimal;
  callbackValue: WooV3Decimal; extremePrice: WooV3Decimal; triggered: boolean; activated: boolean; maker: boolean;
  rootAlgoStatus: string; algoStatus: string; leverage: number; marginMode: 'CROSS' | 'ISOLATED';
}>;

export type WooV3WsPrivatePayload = WooV3WsAccount | readonly WooV3WsBalanceRow[] | readonly WooV3WsPositionRow[] | WooV3WsExecutionReport | readonly WooV3WsAlgoExecutionRow[];

const decodeAccount = (input: unknown): WooV3WsAccount => {
  const row = requireRecord(input, 'WOO V3 websocket account.data');
  const mode = requireEnum(row.m, ['PURE_SPOT', 'MARGIN', 'FUTURE'] as const, 'WOO V3 websocket account.data.m');
  const totalAccountValue = requireDecimal(row.v, 'WOO V3 websocket account.data.v');
  if (mode === 'PURE_SPOT') return Object.freeze({ mode, totalAccountValue });
  const common = {
    marginRatio: requireDecimal(row.mr, 'WOO V3 websocket account.data.mr'),
    openMarginRatio: requireDecimal(row.omr, 'WOO V3 websocket account.data.omr'),
    initialMarginRatio: requireDecimal(row.imr, 'WOO V3 websocket account.data.imr'),
    maintenanceMarginRatio: requireDecimal(row.mmr, 'WOO V3 websocket account.data.mmr'),
    totalCollateral: requireDecimal(row.tc, 'WOO V3 websocket account.data.tc'),
    freeCollateral: requireDecimal(row.fc, 'WOO V3 websocket account.data.fc'),
    totalAccountValue,
  };
  if (mode === 'MARGIN') return Object.freeze({ mode, leverage: requireInteger(row.l, 'WOO V3 websocket account.data.l', 1), ...common });
  return Object.freeze({ mode, defaultMarginMode: requireEnum(row.M, ['CROSS', 'ISOLATED'] as const, 'WOO V3 websocket account.data.M'), positionMode: requireEnum(row.P, ['ONE_WAY', 'HEDGE_MODE'] as const, 'WOO V3 websocket account.data.P'), ...common });
};

const decodeBalanceRows = (input: unknown): readonly WooV3WsBalanceRow[] => {
  const data = requireRecord(input, 'WOO V3 websocket balance.data');
  return Object.freeze(requireArray(data.balances, 'WOO V3 websocket balance.data.balances').map((value, index) => {
    const path = `WOO V3 websocket balance.data.balances[${index}]`;
    const row = requireRecord(value, path);
    return Object.freeze({ token: requireString(row.t, `${path}.t`), holding: requireDecimal(row.h, `${path}.h`), frozen: requireDecimal(row.f, `${path}.f`), interest: requireDecimal(row.i, `${path}.i`), pendingShortQuantity: requireDecimal(row.psq, `${path}.psq`), pendingLongQuantity: requireDecimal(row.plq, `${path}.plq`), staked: requireDecimal(row.s, `${path}.s`), unbonding: requireDecimal(row.u, `${path}.u`), vault: requireDecimal(row.v, `${path}.v`), launchpadVault: requireDecimal(row.l, `${path}.l`), earn: requireDecimal(row.e, `${path}.e`), averageOpenPrice: requireDecimal(row.aop, `${path}.aop`), pnl24h: requireDecimal(row.pnl, `${path}.pnl`), roi24h: requireDecimal(row.roi, `${path}.roi`), fee24h: requireDecimal(row.fee, `${path}.fee`), markPrice: requireDecimal(row.mp, `${path}.mp`), version: requireInteger(row.ver, `${path}.ver`), trialFundBonus: requireDecimal(row.b, `${path}.b`), lossCoverRatio: requireInteger(row.lcr, `${path}.lcr`), timestamp: requireInteger(row.ts, `${path}.ts`) });
  }));
};

const decodePositionRows = (input: unknown): readonly WooV3WsPositionRow[] => {
  const data = requireRecord(input, 'WOO V3 websocket position.data');
  return Object.freeze(requireArray(data.positions, 'WOO V3 websocket position.data.positions').map((value, index) => {
    const path = `WOO V3 websocket position.data.positions[${index}]`;
    const row = requireRecord(value, path);
    return Object.freeze({ symbol: requireSymbol(row.s, `${path}.s`), holding: requireDecimal(row.h, `${path}.h`), pendingLongQuantity: requireDecimal(row.plq, `${path}.plq`), pendingShortQuantity: requireDecimal(row.psq, `${path}.psq`), averageOpenPrice: requireDecimal(row.aop, `${path}.aop`), pnl24h: requireDecimal(row.pnl, `${path}.pnl`), roi24h: requireDecimal(row.roi, `${path}.roi`), fee24h: requireDecimal(row.fee, `${path}.fee`), settlePrice: requireDecimal(row.sp, `${path}.sp`), markPrice: requireDecimal(row.mp, `${path}.mp`), openingTimestamp: requireInteger(row.ot, `${path}.ot`), adlQuantile: requireInteger(row.aq, `${path}.aq`), leverage: requireInteger(row.lv, `${path}.lv`, 1), marginMode: requireEnum(row.m, ['CROSS', 'ISOLATED'] as const, `${path}.m`), positionSide: requireEnum(row.ps, ['BOTH', 'LONG', 'SHORT'] as const, `${path}.ps`), isolatedToken: requireString(row.it, `${path}.it`, true), isolatedAmount: requireDecimal(row.ia, `${path}.ia`), isolatedFrozenLong: requireDecimal(row.il, `${path}.il`), isolatedFrozenShort: requireDecimal(row.is, `${path}.is`), version: requireInteger(row.ver, `${path}.ver`), timestamp: requireInteger(row.ts, `${path}.ts`) });
  }));
};

const decodeExecutionReport = (input: unknown): WooV3WsExecutionReport => {
  const path = 'WOO V3 websocket executionreport.data';
  const row = requireRecord(input, path);
  const messageType = requireInteger(row.mt, `${path}.mt`);
  if (messageType === 1 || messageType === 2) return Object.freeze({ messageType, symbol: requireSymbol(row.s, `${path}.s`), orderId: requireInteger(row.oid, `${path}.oid`, 1), clientOrderId: requireInteger(row.cid, `${path}.cid`), reason: requireString(row.rs, `${path}.rs`, true) });
  if (messageType === 3) return Object.freeze({ messageType, symbol: requireSymbol(row.s, `${path}.s`), reason: requireString(row.rs, `${path}.rs`, true) });
  if (messageType !== 0) throw new Error(`${path}.mt must be 0, 1, 2, or 3`);
  return Object.freeze({ messageType, symbol: requireSymbol(row.s, `${path}.s`), clientOrderId: requireInteger(row.cid, `${path}.cid`), orderId: requireInteger(row.oid, `${path}.oid`, 1), orderType: requireString(row.t, `${path}.t`), side: requireEnum(row.sd, ['BUY', 'SELL'] as const, `${path}.sd`), positionSide: requireEnum(row.ps, ['BOTH', 'LONG', 'SHORT'] as const, `${path}.ps`), quantity: requireDecimal(row.sx, `${path}.sx`), price: requireDecimal(row.px, `${path}.px`), tradeId: requireInteger(row.tid, `${path}.tid`), executedQuantity: requireDecimal(row.esx, `${path}.esx`), executedPrice: requireDecimal(row.epx, `${path}.epx`), fee: requireDecimal(row.f, `${path}.f`), feeAsset: requireString(row.fa, `${path}.fa`, true), totalExecutedQuantity: requireDecimal(row.tesx, `${path}.tesx`), averageExecutedPrice: requireDecimal(row.aepx, `${path}.aepx`), status: requireString(row.ss, `${path}.ss`), reason: requireString(row.rs, `${path}.rs`, true), orderTag: requireString(row.tg, `${path}.tg`, true), totalFee: requireDecimal(row.tf, `${path}.tf`), feeCurrency: requireString(row.tfc, `${path}.tfc`, true), totalRebate: requireDecimal(row.tr, `${path}.tr`), rebateCurrency: requireString(row.trc, `${path}.trc`, true), visibleQuantity: requireDecimal(row.vsx, `${path}.vsx`), timestamp: requireInteger(row.ts, `${path}.ts`), reduceOnly: requireBoolean(row.ro, `${path}.ro`), maker: requireBoolean(row.mk, `${path}.mk`), leverage: requireInteger(row.lv, `${path}.lv`, 1), matchedRpi: requireBoolean(row.rpi, `${path}.rpi`), marginMode: requireEnum(row.m, ['CROSS', 'ISOLATED'] as const, `${path}.m`) });
};

const decodeAlgoExecutionRows = (input: unknown): readonly WooV3WsAlgoExecutionRow[] =>
  Object.freeze(requireArray(input, 'WOO V3 websocket algoexecutionreport.data').map((value, index) => {
    const path = `WOO V3 websocket algoexecutionreport.data[${index}]`;
    const row = requireRecord(value, path);
    return Object.freeze({ symbol: requireSymbol(row.s, `${path}.s`), rootAlgoOrderId: requireInteger(row.raid, `${path}.raid`, 1), parentAlgoOrderId: requireInteger(row.paid, `${path}.paid`), algoOrderId: requireInteger(row.aid, `${path}.aid`, 1), clientAlgoOrderId: requireInteger(row.caid, `${path}.caid`), orderTag: requireString(row.tg, `${path}.tg`, true), status: requireString(row.ss, `${path}.ss`), algoType: requireString(row.at, `${path}.at`), side: requireEnum(row.sd, ['BUY', 'SELL'] as const, `${path}.sd`), positionSide: requireEnum(row.ps, ['BOTH', 'LONG', 'SHORT'] as const, `${path}.ps`), quantity: requireDecimal(row.sx, `${path}.sx`), triggerPrice: requireDecimal(row.trp, `${path}.trp`), triggerPriceType: requireString(row.trpt, `${path}.trpt`), triggerStatus: requireString(row.tss, `${path}.tss`), price: requireDecimal(row.px, `${path}.px`), orderType: requireString(row.t, `${path}.t`), triggerExecutionPrice: requireDecimal(row.tpx, `${path}.tpx`), triggerTimestamp: requireInteger(row.tt, `${path}.tt`), tradeId: requireInteger(row.tid, `${path}.tid`), executedPrice: requireDecimal(row.epx, `${path}.epx`), executedQuantity: requireDecimal(row.esx, `${path}.esx`), fee: requireDecimal(row.f, `${path}.f`), feeAsset: requireString(row.fa, `${path}.fa`, true), reason: requireString(row.rs, `${path}.rs`, true), totalExecutedQuantity: requireDecimal(row.tesx, `${path}.tesx`), averageExecutedPrice: requireDecimal(row.aepx, `${path}.aepx`), totalFee: requireDecimal(row.tf, `${path}.tf`), feeCurrency: requireString(row.tfc, `${path}.tfc`, true), totalRebate: requireDecimal(row.tr, `${path}.tr`), rebateCurrency: requireString(row.trc, `${path}.trc`, true), visibleQuantity: requireDecimal(row.vsx, `${path}.vsx`), timestamp: requireInteger(row.ts, `${path}.ts`), reduceOnly: requireBoolean(row.ro, `${path}.ro`), activatedPrice: requireDecimal(row.apx, `${path}.apx`), callbackRatio: requireDecimal(row.cr, `${path}.cr`), callbackValue: requireDecimal(row.cv, `${path}.cv`), extremePrice: requireDecimal(row.ep, `${path}.ep`), triggered: requireBoolean(row.it, `${path}.it`), activated: requireBoolean(row.ia, `${path}.ia`), maker: requireBoolean(row.im, `${path}.im`), rootAlgoStatus: requireString(row.ras, `${path}.ras`), algoStatus: requireString(row.as, `${path}.as`), leverage: requireInteger(row.lv, `${path}.lv`, 1), marginMode: requireEnum(row.m, ['CROSS', 'ISOLATED'] as const, `${path}.m`) });
  }));

const decodePrivatePayload = (topic: WooV3WsPrivateTopic, input: unknown): Readonly<{ payload: WooV3WsPrivatePayload; blockers: readonly WooV3WsBlocker[] }> => {
  switch (topic) {
    case 'account': return Object.freeze({ payload: decodeAccount(input), blockers: Object.freeze([WOO_V3_WS_BLOCKERS['private-account-replacement-semantics-undocumented']]) });
    case 'balance': return Object.freeze({ payload: decodeBalanceRows(input), blockers: Object.freeze([WOO_V3_WS_BLOCKERS['private-balance-replacement-semantics-undocumented']]) });
    case 'position': return Object.freeze({ payload: decodePositionRows(input), blockers: Object.freeze([WOO_V3_WS_BLOCKERS['private-position-replacement-semantics-undocumented']]) });
    case 'executionreport': return Object.freeze({ payload: decodeExecutionReport(input), blockers: Object.freeze([WOO_V3_WS_BLOCKERS['execution-status-lifecycle-incomplete']]) });
    case 'algoexecutionreport': return Object.freeze({ payload: decodeAlgoExecutionRows(input), blockers: Object.freeze([WOO_V3_WS_BLOCKERS['algo-shape-and-lifecycle-incomplete']]) });
  }
};

export type WooV3WsDecodedFrame =
  | Readonly<{ channel: 'public'; topic: Extract<WooV3WsTopicDescriptor, { channel: 'public' }>; sentTimestamp: number; payload: WooV3WsPublicPayload; blockers: readonly [] }>
  | Readonly<{ channel: 'private'; topic: Extract<WooV3WsTopicDescriptor, { channel: 'private' }>; sentTimestamp: number; payload: WooV3WsPrivatePayload; blockers: readonly WooV3WsBlocker[] }>;

export const decodeWooV3WsDataFrame = (input: unknown): WooV3WsDecodedFrame => {
  const frame = requireRecord(input, 'WOO V3 websocket data frame');
  const topic = parseWooV3WsTopic(frame.topic);
  const sentTimestamp = requireInteger(frame.ts, 'WOO V3 websocket data frame.ts');
  if (topic.channel === 'public') {
    return Object.freeze({
      channel: 'public',
      topic,
      sentTimestamp,
      payload: decodePublicPayload(topic, frame.data),
      blockers: Object.freeze([]) as readonly [],
    });
  }
  const decoded = decodePrivatePayload(topic.kind, frame.data);
  return Object.freeze({ channel: 'private', topic, sentTimestamp, payload: decoded.payload, blockers: decoded.blockers });
};

export type WooV3WsSequenceResult =
  | Readonly<{ accepted: true; nextExpectedPreviousTimestamp: number }>
  | Readonly<{ accepted: false; blocker: 'sequence-gap'; expectedPreviousTimestamp: number; actualPreviousTimestamp: number; requiresFreshSnapshot: true }>;

export const advanceWooV3OrderBookSequence = (
  expectedPreviousTimestamp: number,
  update: WooV3WsOrderBookUpdate,
): WooV3WsSequenceResult => {
  requireInteger(expectedPreviousTimestamp, 'WOO V3 orderbook expected previous timestamp');
  if (update.previousTimestamp !== expectedPreviousTimestamp) {
    return Object.freeze({ accepted: false, blocker: 'sequence-gap', expectedPreviousTimestamp, actualPreviousTimestamp: update.previousTimestamp, requiresFreshSnapshot: true });
  }
  return Object.freeze({ accepted: true, nextExpectedPreviousTimestamp: update.timestamp });
};

export type WooV3WsSubscriptionState = Readonly<{
  channel: WooV3WsChannel;
  generation: number;
  intendedTopics: readonly string[];
  acknowledgedTopics: readonly string[];
  incrementalBooksRequireFreshSnapshot: boolean;
}>;

export const createWooV3WsSubscriptionState = (
  channel: WooV3WsChannel,
  topicsInput: readonly string[],
): WooV3WsSubscriptionState => {
  const topics = requireTopicList(topicsInput, 'WOO V3 intended websocket topics');
  if (topics.length > WOO_V3_WS_LIMITS.maximumTopicsPerConnection) {
    throw new Error(`WOO V3 websocket connection supports at most ${WOO_V3_WS_LIMITS.maximumTopicsPerConnection} topics`);
  }
  assertTopicsMatchChannel(topics, channel, 'WOO V3 intended websocket topics');
  return Object.freeze({
    channel,
    generation: 0,
    intendedTopics: topics,
    acknowledgedTopics: Object.freeze([]),
    incrementalBooksRequireFreshSnapshot:
      channel === 'public' && topics.some((topic) => topic.startsWith('orderbookupdate')),
  });
};

export const planWooV3WsResubscribe = (state: WooV3WsSubscriptionState, idPrefixInput: string): readonly WooV3WsCommand[] => {
  const idPrefix = requireString(idPrefixInput, 'WOO V3 resubscribe id prefix');
  const commands: WooV3WsCommand[] = [];
  for (let index = 0; index < state.intendedTopics.length; index += WOO_V3_WS_LIMITS.maximumTopicsPerCommand) {
    commands.push(
      buildWooV3Subscribe(
        state.channel,
        `${idPrefix}-${commands.length + 1}`,
        state.intendedTopics.slice(index, index + WOO_V3_WS_LIMITS.maximumTopicsPerCommand),
      ),
    );
  }
  return Object.freeze(commands);
};

export const acknowledgeWooV3WsSubscription = (state: WooV3WsSubscriptionState, command: WooV3WsCommand, acknowledgement: WooV3WsCommandAck): WooV3WsSubscriptionState => {
  if (command.channel !== state.channel) {
    throw new Error(`WOO V3 ${command.channel} command cannot update ${state.channel} subscription state`);
  }
  if (command.wire.cmd !== 'SUBSCRIBE') {
    throw new Error('WOO V3 subscription state only accepts SUBSCRIBE acknowledgements');
  }
  decodeWooV3WsCommandAck(acknowledgement, command);
  const acknowledged = new Set(state.acknowledgedTopics);
  acknowledgement.data.forEach((topic) => {
    if (!state.intendedTopics.includes(topic)) throw new Error(`WOO V3 acknowledged unexpected topic ${topic}`);
    acknowledged.add(topic);
  });
  return Object.freeze({ ...state, acknowledgedTopics: Object.freeze(state.intendedTopics.filter((topic) => acknowledged.has(topic))) });
};

export const reconnectWooV3WsSubscription = (state: WooV3WsSubscriptionState): WooV3WsSubscriptionState =>
  Object.freeze({
    channel: state.channel,
    generation: state.generation + 1,
    intendedTopics: state.intendedTopics,
    acknowledgedTopics: Object.freeze([]),
    incrementalBooksRequireFreshSnapshot:
      state.channel === 'public' && state.intendedTopics.some((topic) => topic.startsWith('orderbookupdate')),
  });
