import type { OrderBook } from '../types';

import { updateOrderBookByPrice } from './updateOrderBook';

export type OrderBookSequence = string | number | bigint;
export type SequencedBookLevel = readonly [price: number, amount: number];

export interface SequencedBookFrame {
  sequence: OrderBookSequence;
  bids: readonly SequencedBookLevel[];
  asks: readonly SequencedBookLevel[];
}

export interface SequencedBookDelta extends SequencedBookFrame {
  previousSequence: OrderBookSequence;
}

export type SequencedBookResult =
  | { status: 'applied'; frame: 'snapshot' | 'delta'; sequence: string }
  | { status: 'ignored'; reason: 'awaiting-snapshot' | 'duplicate' | 'stale'; sequence?: string }
  | {
      status: 'recover';
      reason: 'invalid-level' | 'invalid-sequence' | 'missing-snapshot' | 'sequence-gap';
    };

type SequenceState = 'uninitialized' | 'ready' | 'recovering';

const normalizeSequence = (value: OrderBookSequence): bigint | undefined => {
  if (typeof value === 'bigint') return value >= 0n ? value : undefined;
  if (typeof value === 'number') {
    return Number.isSafeInteger(value) && value >= 0 ? BigInt(value) : undefined;
  }

  const trimmed = value.trim();
  if (!/^\d+$/.test(trimmed)) return undefined;

  try {
    return BigInt(trimmed);
  } catch {
    return undefined;
  }
};

const hasValidLevels = (levels: readonly SequencedBookLevel[]) =>
  Array.isArray(levels) &&
  levels.every(
    (level) =>
      Array.isArray(level) &&
      level.length >= 2 &&
      Number.isFinite(level[0]) &&
      level[0] > 0 &&
      Number.isFinite(level[1]) &&
      level[1] >= 0,
  );

/**
 * State machine for snapshot + delta order books with an explicit sequence
 * predecessor contract. It is deliberately transport-agnostic: a `recover`
 * result tells the caller to request a fresh snapshot using the venue's own
 * unsubscribe/resubscribe protocol.
 *
 * The helper clears a discontinuous book and suppresses further deltas until
 * an authoritative snapshot arrives. It never converts sequences through a
 * JavaScript number, so large integer IDs remain exact.
 */
export class SequencedDeltaOrderBook {
  private currentSequenceValue?: bigint;
  private sequenceState: SequenceState = 'uninitialized';

  constructor(private readonly orderBook: OrderBook) {}

  get currentSequence(): string | undefined {
    return this.currentSequenceValue?.toString();
  }

  get isReady(): boolean {
    return this.sequenceState === 'ready';
  }

  get isRecovering(): boolean {
    return this.sequenceState === 'recovering';
  }

  applySnapshot(frame: SequencedBookFrame): SequencedBookResult {
    const sequence = normalizeSequence(frame.sequence);
    if (sequence === undefined) return this.requireRecovery('invalid-sequence');
    if (!hasValidLevels(frame.bids) || !hasValidLevels(frame.asks)) {
      return this.requireRecovery('invalid-level');
    }

    const nextBook: OrderBook = { bids: [], asks: [] };
    this.applyLevels(nextBook, 'bids', frame.bids);
    this.applyLevels(nextBook, 'asks', frame.asks);

    this.orderBook.bids = nextBook.bids;
    this.orderBook.asks = nextBook.asks;
    this.currentSequenceValue = sequence;
    this.sequenceState = 'ready';

    return { status: 'applied', frame: 'snapshot', sequence: sequence.toString() };
  }

  applyDelta(frame: SequencedBookDelta): SequencedBookResult {
    if (this.sequenceState === 'recovering') return { status: 'ignored', reason: 'awaiting-snapshot' };
    if (this.sequenceState !== 'ready' || this.currentSequenceValue === undefined) {
      return this.requireRecovery('missing-snapshot');
    }

    const sequence = normalizeSequence(frame.sequence);
    const previousSequence = normalizeSequence(frame.previousSequence);
    if (sequence === undefined || previousSequence === undefined) {
      return this.requireRecovery('invalid-sequence');
    }

    if (sequence === this.currentSequenceValue) {
      return { status: 'ignored', reason: 'duplicate', sequence: sequence.toString() };
    }
    if (sequence < this.currentSequenceValue) {
      return { status: 'ignored', reason: 'stale', sequence: sequence.toString() };
    }
    if (previousSequence !== this.currentSequenceValue || sequence <= previousSequence) {
      return this.requireRecovery('sequence-gap');
    }
    if (!hasValidLevels(frame.bids) || !hasValidLevels(frame.asks)) {
      return this.requireRecovery('invalid-level');
    }

    this.applyLevels(this.orderBook, 'bids', frame.bids);
    this.applyLevels(this.orderBook, 'asks', frame.asks);
    this.currentSequenceValue = sequence;

    return { status: 'applied', frame: 'delta', sequence: sequence.toString() };
  }

  reset(): void {
    this.clearBook();
    this.currentSequenceValue = undefined;
    this.sequenceState = 'uninitialized';
  }

  private applyLevels(orderBook: OrderBook, side: 'bids' | 'asks', levels: readonly SequencedBookLevel[]): void {
    for (let index = 0; index < levels.length; index++) {
      const [price, amount] = levels[index];
      updateOrderBookByPrice(orderBook, side, price, amount);
    }
  }

  private requireRecovery(reason: Extract<SequencedBookResult, { status: 'recover' }>['reason']): SequencedBookResult {
    if (this.sequenceState === 'recovering') return { status: 'ignored', reason: 'awaiting-snapshot' };

    this.clearBook();
    this.currentSequenceValue = undefined;
    this.sequenceState = 'recovering';
    return { status: 'recover', reason };
  }

  private clearBook(): void {
    this.orderBook.bids = [];
    this.orderBook.asks = [];
  }
}
