import type { ActiveChaser, ActiveTWAP, OrderSide } from '../../types';

import { TwapStatus } from '../../webworker/twap/TwapStatus';

/** A stop that has not shown up in the account's rows is sent again this often: it is idempotent. */
const STOP_RESEND_MS = 5_000;
const NOT_CONFIRMED_MS = 15_000;
const STOP_GIVE_UP_MS = 60_000;
/** After a stop the user was told was uncertain is confirmed, the chase's own end has this long to say how it ended. */
const OUTCOME_WAIT_MS = 10_000;
/** A chase's row and its completion arrive together; past this, a row gone alone is a chase that died. */
const UNENDED_CHASE_MS = 3_000;

type Rows = { activeChasers: ActiveChaser[]; activeTWAPs: ActiveTWAP[] };

/** The `chaser:complete` fields this reads. */
export type ChaseEnd = { id?: string; symbol?: string; size?: number; filled?: number; outcomeTold?: boolean };

/** A stop the user was told was uncertain: they are owed how it ended. */
type Owed = { label: string; done?: () => void };

const chaseOutcome = ({ symbol, size, filled = 0 }: ChaseEnd) => {
  if (!(filled > 0)) return `Chase on ${symbol} stopped with nothing filled.`;
  if (size !== undefined && filled >= size) return `Chase on ${symbol} filled ${filled} before its stop took effect.`;
  return `Chase on ${symbol} stopped after filling ${filled}${size !== undefined ? ` of ${size}` : ''}.`;
};

/** A running chase lost with its engine: no completion will ever say how it ended. */
const unendedChase = (chase: ActiveChaser) => {
  const orders = chase.currentOrderIds?.length ? `order ${chase.currentOrderIds.join(', ')}` : 'order';
  return (
    `Chase on ${chase.symbol} ended without a result: its account reloaded or its engine restarted while it ran. ` +
    `Its ${orders} may still be open, or may have filled: check Positions and Open Orders.`
  );
};

type AlgoWatchHost = {
  rows: () => Rows;
  subscribe: (listener: () => void) => () => void;
  /** Sends the op again through the account's bridge, as the original click did. */
  resend: (op: string, data: unknown) => void;
  tell: (message: string) => void;
};

const chaserStopped = (rows: Rows, id: string) => {
  const row = rows.activeChasers.find((chaser) => chaser.id === id);
  return !row || row.isStopped;
};
const twapStatus = (rows: Rows, id: string) => rows.activeTWAPs.find((twap) => twap.id === id)?.status;
const twapStopped = (rows: Rows, id: string) => {
  const status = twapStatus(rows, id);
  return status === undefined || status === TwapStatus.Completed;
};

/**
 * Algo ops are one way: the server's only answer is the account's rows, so a frame
 * lost with a dying socket used to say nothing. This watches those rows. A stop is
 * sent again until they confirm it; a start, resume or pause is never sent again,
 * and the user is told when it is not confirmed.
 */
export class AlgoControlWatch {
  private readonly watches = new Set<() => void>();
  private resending = false;

  /** Chases the rows show running, and those whose completion came. */
  private running = new Map<string, ActiveChaser>();
  private readonly ended = new Set<string>();
  private readonly unended = new Map<string, ReturnType<typeof setTimeout>>();
  private readonly stopTracking: () => void;

  constructor(private readonly host: AlgoWatchHost) {
    this.stopTracking = host.subscribe(() => this.trackChases());
    this.trackChases();
  }

  /** A chase whose row goes with no completion died with its engine: say so, naming its order. */
  private trackChases() {
    const rows = new Map(this.host.rows().activeChasers.map((chase) => [chase.id, chase]));
    for (const [id, chase] of this.running) {
      if (rows.has(id) || this.unended.has(id)) continue;
      if (this.ended.delete(id) || this.owed.has(id)) continue;
      this.unended.set(
        id,
        setTimeout(() => {
          this.unended.delete(id);
          if (!this.ended.delete(id)) this.host.tell(unendedChase(chase));
        }, UNENDED_CHASE_MS),
      );
    }
    for (const id of rows.keys()) {
      const timer = this.unended.get(id);
      if (timer !== undefined) clearTimeout(timer);
      this.unended.delete(id);
    }
    this.running = rows;
  }

  /** The op just went out; call for every chaser or TWAP control op. */
  observe(op: string, data: unknown) {
    if (this.resending) return;
    const id = typeof data === 'string' ? data : undefined;
    const rows = this.host.rows();
    switch (op) {
      case 'chaser:terminate':
        if (id && !chaserStopped(rows, id)) this.watchStop(op, id, 'Chase', (next) => chaserStopped(next, id));
        break;
      case 'twap:stop':
        if (id && !twapStopped(rows, id)) this.watchStop(op, id, 'TWAP', (next) => twapStopped(next, id));
        break;
      case 'twap:pause':
        if (id && twapStatus(rows, id) === TwapStatus.Running) {
          this.watchOnce('TWAP pause', (next) => twapStatus(next, id) !== TwapStatus.Running);
        }
        break;
      case 'twap:resume':
        if (id && twapStatus(rows, id) === TwapStatus.Paused) {
          this.watchOnce('TWAP resume', (next) => twapStatus(next, id) !== TwapStatus.Paused);
        }
        break;
      case 'chaser:start':
      case 'twap:start': {
        const { symbol, side } = (data ?? {}) as { symbol?: string; side?: OrderSide };
        if (op === 'twap:start') {
          const before = new Set(rows.activeTWAPs.map((twap) => twap.id));
          this.watchOnce(`TWAP start for ${symbol}`, (next) => next.activeTWAPs.some((twap) => !before.has(twap.id)));
        } else {
          const before = new Set(rows.activeChasers.map((chaser) => chaser.id));
          this.watchOnce(
            `Chase start for ${symbol}`,
            (next) =>
              this.completedChaseSymbols.delete(symbol ?? '') ||
              next.activeChasers.some((chaser) => !before.has(chaser.id) && chaser.symbol === symbol && chaser.side === side),
          );
        }
        break;
      }
      default:
    }
  }

  private readonly completedChaseSymbols = new Set<string>();

  /** A chase can finish between two row updates, leaving no row to see. */
  chaseCompleted(event: ChaseEnd) {
    if (event.id !== undefined) {
      const timer = this.unended.get(event.id);
      if (timer !== undefined) clearTimeout(timer);
      this.unended.delete(event.id);
      if (this.running.has(event.id)) this.ended.add(event.id);
    }
    if (event.symbol) {
      this.completedChaseSymbols.add(event.symbol);
      for (const check of [...this.checks]) check();
      this.completedChaseSymbols.delete(event.symbol);
    }
    const owed = event.id === undefined ? undefined : this.owed.get(event.id);
    if (!owed || event.id === undefined) return;
    this.settle(event.id);
    // A stop the chaser had to retry ends with its own message: one outcome, not two.
    if (!event.outcomeTold) this.host.tell(chaseOutcome(event));
  }

  private readonly owed = new Map<string, Owed>();

  private owe(id: string, label: string) {
    if (!this.owed.has(id)) this.owed.set(id, { label });
  }

  private settle(id: string) {
    this.owed.get(id)?.done?.();
    this.owed.delete(id);
  }

  /** The stop landed after the user was told it might not: say how it ended. */
  private confirmedLate(id: string) {
    const owed = this.owed.get(id);
    if (!owed) return;
    if (owed.label !== 'Chase') {
      this.settle(id);
      this.host.tell(`${owed.label} stop is confirmed now.`);
      return;
    }
    // A chase says how it ended in its own completion; a confirmation alone cannot.
    const timer = setTimeout(() => {
      this.settle(id);
      this.host.tell('Chase stop is confirmed, but whether it filled is not known: check Positions and Open Orders.');
    }, OUTCOME_WAIT_MS);
    owed.done = () => clearTimeout(timer);
  }

  private readonly checks = new Set<() => void>();

  private watch(confirmed: (rows: Rows) => boolean, timers: Array<[number, () => void]>) {
    const handles = timers.map(([ms, run]) => setTimeout(run, ms));
    const check = () => {
      if (confirmed(this.host.rows())) stop();
    };
    const unsubscribe = this.host.subscribe(check);
    const stop = () => {
      handles.forEach(clearTimeout);
      unsubscribe();
      this.checks.delete(check);
      this.watches.delete(stop);
    };
    this.checks.add(check);
    this.watches.add(stop);
    return stop;
  }

  private watchOnce(what: string, confirmed: (rows: Rows) => boolean) {
    const stop = this.watch(confirmed, [
      [
        NOT_CONFIRMED_MS,
        () => {
          stop();
          this.host.tell(
            `${what} was not confirmed after ${NOT_CONFIRMED_MS / 1000}s and was not sent again. ` +
              'It may still arrive when the connection returns; check before trying again.',
          );
        },
      ],
    ]);
  }

  private readonly stopping = new Set<string>();

  private watchStop(op: string, id: string, label: string, confirmed: (rows: Rows) => boolean) {
    // A second click on a stop already being retried adds nothing.
    if (this.stopping.has(id)) return;
    this.stopping.add(id);
    const timers: Array<[number, () => void]> = [
      [
        NOT_CONFIRMED_MS,
        () => {
          this.owe(id, label);
          this.host.tell(`${label} stop is not confirmed yet; retrying. It may still be running.`);
        },
      ],
    ];
    for (let at = STOP_RESEND_MS; at < STOP_GIVE_UP_MS; at += STOP_RESEND_MS) {
      timers.push([
        at,
        () => {
          this.resending = true;
          try {
            this.host.resend(op, id);
          } finally {
            this.resending = false;
          }
        },
      ]);
    }
    timers.push([
      STOP_GIVE_UP_MS,
      () => {
        stop();
        this.stopping.delete(id);
        this.owe(id, label);
        this.host.tell(
          `${label} stop was NOT confirmed after ${STOP_GIVE_UP_MS / 1000}s. It may still be running: check it and stop it again.`,
        );
        // Sent no more, but still watched: the user is told how it ends when the connection returns.
        const owed = this.owed.get(id);
        const unwatch = this.watch((rows) => {
          const done = confirmed(rows);
          if (done) this.confirmedLate(id);
          return done;
        }, []);
        if (owed) owed.done = unwatch;
      },
    ]);
    const stop = this.watch((rows) => {
      const done = confirmed(rows);
      if (done) {
        this.stopping.delete(id);
        this.confirmedLate(id);
      }
      return done;
    }, timers);
  }

  dispose() {
    this.stopTracking();
    this.unended.forEach(clearTimeout);
    this.unended.clear();
    for (const stop of [...this.watches]) stop();
    for (const id of [...this.owed.keys()]) this.settle(id);
    this.stopping.clear();
  }
}
