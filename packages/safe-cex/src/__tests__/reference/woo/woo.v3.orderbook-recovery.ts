import type { OrderBook } from '../../../types';
import type { SequencedBookDelta, SequencedBookLevel } from '../../../utils/sequencedDeltaOrderBook';
import type { WooV3MappedOrderBookSnapshot } from './woo.v3.public';
import type { WooV3WsBookDepth } from './woo.v3.websocket';

import { calcOrderBookTotal } from '../../../utils/orderbook';
import { SequencedDeltaOrderBook } from '../../../utils/sequencedDeltaOrderBook';
import { decodeWooV3OrderBook, mapWooV3OrderBook } from './woo.v3.public';

/**
 * Transport-free recovery state for WOO V3 incremental books. The live Woo
 * websocket deliberately does not import this module while the public WS gate
 * is false.
 */

type UnknownRecord = Record<string, unknown>;

type NormalizedUpdate = Readonly<{
  symbol: string;
  previousTimestamp: number;
  timestamp: number;
  asks: readonly SequencedBookLevel[];
  bids: readonly SequencedBookLevel[];
  fingerprint: string;
}>;

export type WooV3RecoveredOrderBook = WooV3MappedOrderBookSnapshot &
  Readonly<{
    /** REST snapshot provenance retained after the live timestamp advances. */
    snapshotTimestamp: number;
  }>;

/**
 * WOO publishes a maximum incremental-book depth of 500. Its docs do not give
 * a snapshot-latency/update-cadence bound, so reuse that largest documented
 * finite scale as a conservative queue ceiling instead of inventing a time
 * window or permitting unbounded growth.
 */
export const WOO_V3_ORDERBOOK_MAX_BUFFERED_UPDATES = 500;

export type WooV3OrderBookRecoveryResult =
  | Readonly<{
      status: 'buffered';
      bufferedUpdates: number;
      publication: undefined;
    }>
  | Readonly<{
      status: 'awaiting-join';
      snapshotTimestamp: number;
      staleUpdatesDiscarded: number;
      publication: undefined;
    }>
  | Readonly<{
      status: 'joined';
      snapshotTimestamp: number;
      updateTimestamp: number;
      appliedUpdates: number;
      staleUpdatesDiscarded: number;
      duplicatesIgnored: number;
      publication: WooV3RecoveredOrderBook;
    }>
  | Readonly<{
      status: 'applied';
      snapshotTimestamp: number;
      updateTimestamp: number;
      publication: WooV3RecoveredOrderBook;
    }>
  | Readonly<{
      status: 'ignored';
      reason: 'duplicate' | 'stale';
      updateTimestamp: number;
      publication: undefined;
    }>
  | Readonly<{
      status: 'ignored';
      reason: 'stale-snapshot-request';
      publication: undefined;
    }>
  | Readonly<{
      status: 'recover';
      reason:
        | 'invalid-snapshot'
        | 'invalid-delta'
        | 'conflicting-duplicate'
        | 'out-of-order-prejoin-update'
        | 'frame-level-overflow'
        | 'buffer-overflow'
        | 'missing-join'
        | 'sequence-gap';
      requiresFreshSnapshot: true;
      publication: undefined;
    }>
  | Readonly<{
      status: 'reset';
      reason: 'reconnect';
      requiresFreshSnapshot: true;
      publication: undefined;
    }>;

type RecoveryPhase = 'awaiting-snapshot' | 'awaiting-join' | 'joined';

const DECIMAL_PATTERN = /^-?(?:\d+(?:\.\d*)?|\.\d+)$/;

const isRecord = (value: unknown): value is UnknownRecord =>
  value !== null && typeof value === 'object' && !Array.isArray(value);

const requirePositiveTimestamp = (value: unknown, path: string): number => {
  if (!Number.isSafeInteger(value) || (value as number) <= 0) {
    throw new Error(`${path} must be a positive safe integer millisecond timestamp`);
  }
  return value as number;
};

const requireNumericLevel = (
  input: unknown,
  path: string,
): Readonly<{ numeric: SequencedBookLevel; wire: readonly [string, string] }> => {
  if (!Array.isArray(input) || input.length !== 2) throw new Error(`${path} must be a price/quantity tuple`);
  const [wirePrice, wireQuantity] = input;
  if (
    typeof wirePrice !== 'string' ||
    !DECIMAL_PATTERN.test(wirePrice) ||
    !/[1-9]/.test(wirePrice) ||
    wirePrice.startsWith('-')
  ) {
    throw new Error(`${path}[0] must be a positive decimal string`);
  }
  if (typeof wireQuantity !== 'string' || !DECIMAL_PATTERN.test(wireQuantity) || wireQuantity.startsWith('-')) {
    throw new Error(`${path}[1] must be a non-negative decimal string`);
  }

  const price = Number(wirePrice);
  const quantity = Number(wireQuantity);
  if (!Number.isFinite(price) || price <= 0) throw new Error(`${path}[0] must map to a positive finite number`);
  if (!Number.isFinite(quantity) || quantity < 0 || (quantity === 0 && /[1-9]/.test(wireQuantity))) {
    throw new Error(`${path}[1] must map losslessly to a non-negative finite number`);
  }

  return Object.freeze({
    numeric: Object.freeze([price, quantity] as const),
    wire: Object.freeze([wirePrice, wireQuantity] as const),
  });
};

const normalizeSide = (
  input: unknown,
  path: string,
): Readonly<{ numeric: readonly SequencedBookLevel[]; wire: readonly (readonly [string, string])[] }> => {
  if (!Array.isArray(input)) throw new Error(`${path} must be an array`);
  const decoded = input.map((level, index) => requireNumericLevel(level, `${path}[${index}]`));
  return Object.freeze({
    numeric: Object.freeze(decoded.map((level) => level.numeric)),
    wire: Object.freeze(decoded.map((level) => level.wire)),
  });
};

const normalizeUpdate = (input: unknown, expectedSymbol: string): NormalizedUpdate => {
  if (!isRecord(input)) throw new Error('WOO V3 orderbook delta must be an object');
  if (input.symbol !== expectedSymbol) throw new Error('WOO V3 orderbook delta symbol does not match its coordinator');
  const previousTimestamp = requirePositiveTimestamp(input.previousTimestamp, 'WOO V3 orderbook delta.prevTs');
  const timestamp = requirePositiveTimestamp(input.timestamp, 'WOO V3 orderbook delta.ts');
  if (timestamp <= previousTimestamp) throw new Error('WOO V3 orderbook delta.ts must follow prevTs');
  const asks = normalizeSide(input.asks, 'WOO V3 orderbook delta.asks');
  const bids = normalizeSide(input.bids, 'WOO V3 orderbook delta.bids');
  return Object.freeze({
    symbol: expectedSymbol,
    previousTimestamp,
    timestamp,
    asks: asks.numeric,
    bids: bids.numeric,
    fingerprint: JSON.stringify([expectedSymbol, previousTimestamp, timestamp, asks.wire, bids.wire]),
  });
};

const snapshotLevels = (snapshot: WooV3MappedOrderBookSnapshot, side: 'bids' | 'asks'): readonly SequencedBookLevel[] =>
  Object.freeze(snapshot[side].map((level) => Object.freeze([level.price, level.amount] as const)));

/**
 * Buffers decoded WOO V3 deltas until a REST snapshot can join the exact
 * `prevTs === snapshot.timestamp` boundary. It exposes no mutable book and
 * returns no publication before that boundary is proven.
 */
export class WooV3OrderBookRecoveryCoordinator {
  private readonly orderBook: OrderBook = { bids: [], asks: [] };
  private readonly sequencedBook = new SequencedDeltaOrderBook(this.orderBook);
  private readonly bufferedUpdates: NormalizedUpdate[] = [];
  private phase: RecoveryPhase = 'awaiting-snapshot';
  private snapshotTimestamp?: number;
  private currentTimestamp?: number;
  private currentFingerprint?: string;
  private nextSnapshotRequestToken = 1;
  private activeSnapshotRequestToken?: number;

  constructor(
    private readonly symbol: string,
    private readonly depth: WooV3WsBookDepth,
  ) {
    if (typeof symbol !== 'string' || symbol.length === 0) {
      throw new Error('WOO V3 orderbook coordinator symbol must be a non-empty string');
    }
    if (depth !== 50 && depth !== 200 && depth !== 500) {
      throw new Error('WOO V3 orderbook coordinator depth must be 50, 200, or 500');
    }
  }

  get requiresFreshSnapshot(): boolean {
    return this.phase !== 'joined';
  }

  /**
   * Starts one logical REST snapshot request. A newer request invalidates every
   * older token, and the matching response consumes the token exactly once.
   */
  beginSnapshotRequest(): number {
    if (!Number.isSafeInteger(this.nextSnapshotRequestToken)) {
      throw new Error('WOO V3 orderbook snapshot request token space exhausted');
    }
    const token = this.nextSnapshotRequestToken;
    this.nextSnapshotRequestToken += 1;
    this.activeSnapshotRequestToken = token;
    return token;
  }

  pushUpdate(input: unknown): WooV3OrderBookRecoveryResult {
    if (
      isRecord(input) &&
      ((Array.isArray(input.asks) && input.asks.length > this.depth) ||
        (Array.isArray(input.bids) && input.bids.length > this.depth))
    ) {
      return this.recover('frame-level-overflow');
    }

    let update: NormalizedUpdate;
    try {
      update = normalizeUpdate(input, this.symbol);
    } catch {
      return this.recover('invalid-delta');
    }

    if (this.phase === 'awaiting-snapshot') return this.bufferBeforeSnapshot(update);
    if (this.phase === 'awaiting-join') return this.joinAfterSnapshot(update);
    return this.applyJoinedUpdate(update);
  }

  acceptRestSnapshot(input: unknown, snapshotRequestToken: unknown): WooV3OrderBookRecoveryResult {
    if (
      this.activeSnapshotRequestToken === undefined ||
      !Number.isSafeInteger(snapshotRequestToken) ||
      (snapshotRequestToken as number) <= 0 ||
      snapshotRequestToken !== this.activeSnapshotRequestToken
    ) {
      return Object.freeze({
        status: 'ignored',
        reason: 'stale-snapshot-request',
        publication: undefined,
      } as const);
    }
    this.activeSnapshotRequestToken = undefined;

    let snapshot: WooV3MappedOrderBookSnapshot;
    try {
      snapshot = mapWooV3OrderBook(decodeWooV3OrderBook(input));
    } catch {
      return this.recover('invalid-snapshot');
    }
    if (snapshot.bids.length > this.depth || snapshot.asks.length > this.depth) {
      return this.recover('frame-level-overflow');
    }

    this.sequencedBook.reset();
    const snapshotResult = this.sequencedBook.applySnapshot({
      sequence: snapshot.timestamp,
      bids: snapshotLevels(snapshot, 'bids'),
      asks: snapshotLevels(snapshot, 'asks'),
    });
    if (snapshotResult.status !== 'applied') return this.recover('invalid-snapshot');

    this.snapshotTimestamp = snapshot.timestamp;
    this.currentTimestamp = snapshot.timestamp;
    this.currentFingerprint = undefined;
    this.phase = 'awaiting-join';

    const queued = this.bufferedUpdates.splice(0);
    let staleUpdatesDiscarded = 0;
    let duplicatesIgnored = 0;
    let appliedUpdates = 0;

    for (const update of queued) {
      if (update.timestamp <= snapshot.timestamp) {
        staleUpdatesDiscarded += 1;
        continue;
      }

      if (this.phase === 'awaiting-join') {
        if (update.previousTimestamp !== snapshot.timestamp) return this.recover('missing-join');
        if (!this.applyToSequencer(update)) return this.recover('sequence-gap');
        this.phase = 'joined';
        appliedUpdates += 1;
        continue;
      }

      if (update.timestamp === this.currentTimestamp) {
        if (update.fingerprint !== this.currentFingerprint) return this.recover('conflicting-duplicate');
        duplicatesIgnored += 1;
        continue;
      }
      if ((this.currentTimestamp ?? 0) > update.timestamp) {
        staleUpdatesDiscarded += 1;
        continue;
      }
      if (!this.applyToSequencer(update)) return this.recover('sequence-gap');
      appliedUpdates += 1;
    }

    if (this.phase === 'awaiting-join') {
      return Object.freeze({
        status: 'awaiting-join',
        snapshotTimestamp: snapshot.timestamp,
        staleUpdatesDiscarded,
        publication: undefined,
      });
    }

    return Object.freeze({
      status: 'joined',
      snapshotTimestamp: snapshot.timestamp,
      updateTimestamp: this.currentTimestamp!,
      appliedUpdates,
      staleUpdatesDiscarded,
      duplicatesIgnored,
      publication: this.createPublication(),
    });
  }

  reconnect(): WooV3OrderBookRecoveryResult {
    this.reset();
    return Object.freeze({
      status: 'reset',
      reason: 'reconnect',
      requiresFreshSnapshot: true,
      publication: undefined,
    });
  }

  private bufferBeforeSnapshot(update: NormalizedUpdate): WooV3OrderBookRecoveryResult {
    const latest = this.bufferedUpdates[this.bufferedUpdates.length - 1];
    if (latest && update.timestamp === latest.timestamp) {
      if (update.fingerprint !== latest.fingerprint) return this.recover('conflicting-duplicate');
      return Object.freeze({
        status: 'ignored',
        reason: 'duplicate',
        updateTimestamp: update.timestamp,
        publication: undefined,
      });
    }
    if (latest && update.timestamp < latest.timestamp) return this.recover('out-of-order-prejoin-update');
    if (this.bufferedUpdates.length === WOO_V3_ORDERBOOK_MAX_BUFFERED_UPDATES) {
      return this.recover('buffer-overflow');
    }
    this.bufferedUpdates.push(update);
    return Object.freeze({ status: 'buffered', bufferedUpdates: this.bufferedUpdates.length, publication: undefined });
  }

  private joinAfterSnapshot(update: NormalizedUpdate): WooV3OrderBookRecoveryResult {
    if (update.timestamp <= this.snapshotTimestamp!) {
      return Object.freeze({
        status: 'ignored',
        reason: 'stale',
        updateTimestamp: update.timestamp,
        publication: undefined,
      });
    }
    if (update.previousTimestamp !== this.snapshotTimestamp) return this.recover('missing-join');
    if (!this.applyToSequencer(update)) return this.recover('sequence-gap');
    this.phase = 'joined';
    return Object.freeze({
      status: 'joined',
      snapshotTimestamp: this.snapshotTimestamp!,
      updateTimestamp: update.timestamp,
      appliedUpdates: 1,
      staleUpdatesDiscarded: 0,
      duplicatesIgnored: 0,
      publication: this.createPublication(),
    });
  }

  private applyJoinedUpdate(update: NormalizedUpdate): WooV3OrderBookRecoveryResult {
    if (update.timestamp === this.currentTimestamp) {
      if (update.fingerprint !== this.currentFingerprint) return this.recover('conflicting-duplicate');
      return Object.freeze({
        status: 'ignored',
        reason: 'duplicate',
        updateTimestamp: update.timestamp,
        publication: undefined,
      });
    }
    if (update.timestamp < this.currentTimestamp!) {
      return Object.freeze({
        status: 'ignored',
        reason: 'stale',
        updateTimestamp: update.timestamp,
        publication: undefined,
      });
    }
    if (!this.applyToSequencer(update)) return this.recover('sequence-gap');
    return Object.freeze({
      status: 'applied',
      snapshotTimestamp: this.snapshotTimestamp!,
      updateTimestamp: update.timestamp,
      publication: this.createPublication(),
    });
  }

  private applyToSequencer(update: NormalizedUpdate): boolean {
    const frame: SequencedBookDelta = {
      sequence: update.timestamp,
      previousSequence: update.previousTimestamp,
      bids: update.bids,
      asks: update.asks,
    };
    const result = this.sequencedBook.applyDelta(frame);
    if (result.status !== 'applied') return false;
    this.currentTimestamp = update.timestamp;
    this.currentFingerprint = update.fingerprint;
    calcOrderBookTotal(this.orderBook);
    return true;
  }

  private createPublication(): WooV3RecoveredOrderBook {
    const cloneSide = (side: 'bids' | 'asks') => this.orderBook[side].map((level) => Object.freeze({ ...level }));
    return Object.freeze({
      snapshotTimestamp: this.snapshotTimestamp!,
      timestamp: this.currentTimestamp!,
      bids: cloneSide('bids'),
      asks: cloneSide('asks'),
    });
  }

  private recover(
    reason: Extract<WooV3OrderBookRecoveryResult, { status: 'recover' }>['reason'],
  ): WooV3OrderBookRecoveryResult {
    this.reset();
    return Object.freeze({ status: 'recover', reason, requiresFreshSnapshot: true, publication: undefined });
  }

  private reset(): void {
    this.sequencedBook.reset();
    this.bufferedUpdates.splice(0);
    this.phase = 'awaiting-snapshot';
    this.snapshotTimestamp = undefined;
    this.currentTimestamp = undefined;
    this.currentFingerprint = undefined;
    this.activeSnapshotRequestToken = undefined;
  }
}
