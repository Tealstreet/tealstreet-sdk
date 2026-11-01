import type { ActiveChaser, ActiveTWAP, OrderSide } from '../../types';

import { TwapStatus } from '../../webworker/twap/TwapStatus';

/** A stop that has not shown up in the account's rows is sent again this often: it is idempotent. */
const STOP_RESEND_MS = 5_000;
const NOT_CONFIRMED_MS = 15_000;
const STOP_GIVE_UP_MS = 60_000;

type Rows = { activeChasers: ActiveChaser[]; activeTWAPs: ActiveTWAP[] };

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

  constructor(private readonly host: AlgoWatchHost) {}

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
  chaseCompleted(symbol: string) {
    this.completedChaseSymbols.add(symbol);
    for (const check of [...this.checks]) check();
    this.completedChaseSymbols.delete(symbol);
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
      [NOT_CONFIRMED_MS, () => this.host.tell(`${label} stop is not confirmed yet; retrying. It may still be running.`)],
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
        this.host.tell(
          `${label} stop was NOT confirmed after ${STOP_GIVE_UP_MS / 1000}s. It may still be running: check it and stop it again.`,
        );
      },
    ]);
    const stop = this.watch((rows) => {
      const done = confirmed(rows);
      if (done) this.stopping.delete(id);
      return done;
    }, timers);
  }

  dispose() {
    for (const stop of [...this.watches]) stop();
    this.stopping.clear();
  }
}
