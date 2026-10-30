import type { EngineEvent } from '@tealstreet/safe-cex-wasm/host';

import { LogSeverity } from './types';

/**
 * Stages of one user write, in order. `received` exists only when the caller and
 * the engine are in different threads; `signed` only for a delegated signer
 * (in-engine signing is inside `accepted`).
 */
export const WRITE_STAGES = [
  'called',
  'received',
  'accepted',
  'signed',
  'sent',
  'response',
  'reply',
  'resolved',
] as const;
export type WriteStage = (typeof WRITE_STAGES)[number];
export type WriteMarks = Partial<Record<WriteStage, number>>;
export type EngineWriteStage = 'accepted' | 'signed' | 'sent' | 'response' | 'reply';

/** Epoch milliseconds at sub-millisecond resolution, comparable across the main thread and workers. */
export const writeClock = () => performance.timeOrigin + performance.now();

const round = (ms: number) => Math.round(ms * 10) / 10;
const capitalized = (stage: string) => stage[0].toUpperCase() + stage.slice(1);

export class WriteTiming {
  constructor(
    readonly op: string,
    readonly marks: WriteMarks = {},
  ) {}

  /** First stamp wins: a batch's `sent` and `response` are its first request's. */
  mark = (stage: WriteStage, at = writeClock()) => {
    this.marks[stage] ??= at;
  };

  event(outcome: 'ok' | 'error'): EngineEvent {
    const present = WRITE_STAGES.filter((stage) => typeof this.marks[stage] === 'number');
    const fields: EngineEvent['fields'] = { op: this.op, outcome };
    const spans: string[] = [];
    for (let i = 1; i < present.length; i++) {
      const delta = round(this.marks[present[i]]! - this.marks[present[i - 1]]!);
      fields[`${present[i - 1]}To${capitalized(present[i])}Ms`] = delta;
      spans.push(`${present[i - 1]}>${present[i]} ${delta}`);
    }
    const total = present.length ? round(this.marks[present[present.length - 1]]! - this.marks[present[0]]!) : 0;
    fields.totalMs = total;
    return {
      level: 'info',
      code: 'write.timing',
      message: `${this.op} ${outcome} ${total}ms: ${spans.join(', ')}`,
      fields,
    };
  }
}

/** User writes whose caller may be in another thread; their RPC carries the marks both ways. */
export const TIMED_WRITE_METHODS: ReadonlySet<string> = new Set([
  'placeOrder',
  'placeOrders',
  'placeOrderWithClientOrderId',
  'updateOrder',
  'cancelOrders',
  'cancelSymbolOrders',
]);

let handedDown: WriteTiming | undefined;

/** Hands `timing` to the write method that `start` invokes synchronously. */
export const runWithWriteTiming = <T>(timing: WriteTiming, start: () => T): T => {
  handedDown = timing;
  try {
    return start();
  } finally {
    handedDown = undefined;
  }
};

/**
 * Called first thing in a write method. A timing handed down by a caller in
 * another thread is not owned: that caller stamps `resolved` and emits.
 */
export const claimWriteTiming = (op: string): { timing: WriteTiming; owned: boolean } => {
  const timing = handedDown;
  handedDown = undefined;
  if (timing) return { timing, owned: false };
  const owned = new WriteTiming(op);
  owned.mark('called');
  return { timing: owned, owned: true };
};

/** The route every info engine event takes: the `engine-event` stream and the exchange log. */
export const emitWriteTiming = (
  exchange: {
    emitter: { emit: (event: 'engine-event', payload: EngineEvent) => unknown };
    log: (message: string, severity: LogSeverity, engineEvent: EngineEvent) => void;
  },
  timing: WriteTiming,
  outcome: 'ok' | 'error',
) => {
  try {
    timing.mark('resolved');
    const event = timing.event(outcome);
    exchange.emitter.emit('engine-event', event);
    exchange.log(event.message, LogSeverity.Info, event);
  } catch {
    /* Diagnostics cannot change a write's outcome. */
  }
};
