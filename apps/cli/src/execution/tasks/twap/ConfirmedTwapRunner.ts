import type { ConfirmedOrderHandle } from '../../confirmed';
import { MarketTwapTask, type MarketTwapReport } from './MarketTwapTask';

const LOOKUP_FLOOR_MS = 2_000;
const CADENCE_MS = 100;

export class ConfirmedTwapRunner {
  private readonly tasks: MarketTwapTask[] = [];
  private readonly nowMs: () => number;
  private readonly onChange: () => void;
  private readonly lastLookupAt = new Map<ConfirmedOrderHandle, number>();
  private readonly lookupInFlight = new Set<ConfirmedOrderHandle>();
  private readonly placementInFlight = new Set<MarketTwapTask>();
  private timer: ReturnType<typeof setTimeout> | undefined;
  private lookupCursor = 0;
  private cancelled = false;

  constructor(input: { readonly nowMs?: () => number; readonly onChange?: () => void } = {}) {
    this.nowMs = input.nowMs ?? (() => Date.now());
    this.onChange = input.onChange ?? (() => undefined);
  }

  add(task: MarketTwapTask): void {
    if (this.cancelled) throw new Error('Confirmed TWAP runner is quiesced');
    this.tasks.push(task);
    this.schedule(0);
  }

  hasLiveWork(): boolean {
    return (
      this.placementInFlight.size > 0 ||
      this.lookupInFlight.size > 0 ||
      this.tasks.some((task) => task.hasScheduledWork() || task.hasUnresolved())
    );
  }

  async waitUntil(task: MarketTwapTask, deadlineAtMs: number): Promise<void> {
    while (
      this.nowMs() < deadlineAtMs &&
      (this.placementInFlight.has(task) || task.hasUnresolved())
    ) {
      await sleep(Math.min(50, Math.max(1, deadlineAtMs - this.nowMs())));
    }
  }

  async quiesce(deadlineAtMs: number): Promise<readonly MarketTwapReport[]> {
    for (const task of this.tasks) task.stop();
    this.schedule(0);
    while (this.nowMs() < deadlineAtMs) {
      if (
        this.placementInFlight.size === 0 &&
        this.lookupInFlight.size === 0 &&
        this.tasks.every((task) => !task.hasUnresolved())
      ) {
        break;
      }
      await sleep(Math.min(50, Math.max(1, deadlineAtMs - this.nowMs())));
    }
    this.cancelled = true;
    clearTimeout(this.timer);
    this.timer = undefined;
    return this.tasks.map((task) => task.report());
  }

  private schedule(delayMs: number): void {
    if (this.cancelled || this.timer !== undefined) return;
    this.timer = setTimeout(() => {
      this.timer = undefined;
      this.tick();
    }, delayMs);
  }

  private tick(): void {
    if (this.cancelled) return;
    const now = this.nowMs();

    for (const task of this.tasks) {
      if (!task.hasDueWork(now) || this.placementInFlight.has(task)) continue;
      this.placementInFlight.add(task);
      void task.placeDueSlices(now).then(
        () => this.onChange(),
        () => {
          task.pause();
          this.onChange();
        },
      ).finally(() => {
        this.placementInFlight.delete(task);
        this.onChange();
      });
    }

    const candidates: Array<{ task: MarketTwapTask; handle: ConfirmedOrderHandle }> = [];
    for (const task of this.tasks) {
      for (const handle of task.unresolvedHandles()) candidates.push({ task, handle });
    }
    if (candidates.length > 0) {
      for (let offset = 0; offset < candidates.length; offset += 1) {
        const index = (this.lookupCursor + offset) % candidates.length;
        const candidate = candidates[index]!;
        const last = this.lastLookupAt.get(candidate.handle);
        if (
          this.lookupInFlight.has(candidate.handle) ||
          (last !== undefined && now - last < LOOKUP_FLOOR_MS)
        ) {
          continue;
        }
        this.lookupCursor = (index + 1) % candidates.length;
        this.lastLookupAt.set(candidate.handle, now);
        this.lookupInFlight.add(candidate.handle);
        void candidate.task.refreshHandle(candidate.handle).then(
          () => this.onChange(),
          () => {
            candidate.task.pause();
            this.onChange();
          },
        ).finally(() => {
          this.lookupInFlight.delete(candidate.handle);
          this.onChange();
        });
        break;
      }
    }

    this.onChange();
    if (this.hasLiveWork()) this.schedule(CADENCE_MS);
  }
}

const sleep = (ms: number): Promise<void> => {
  const { promise, resolve } = Promise.withResolvers<void>();
  setTimeout(resolve, ms);
  return promise;
};
