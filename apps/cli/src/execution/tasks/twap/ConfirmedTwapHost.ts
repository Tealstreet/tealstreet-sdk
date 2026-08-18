import type { Exchange } from '@tealstreet/safe-cex/src/exchanges/base';

import { createConfirmedOrders, type ConfirmedAuditSink } from '../../confirmed';
import { ConfirmedTwapRunner } from './ConfirmedTwapRunner';
import { MarketTwapTask, type MarketTwapReport, type MarketTwapSpec } from './MarketTwapTask';

export const CONFIRMED_OUTCOME_GRACE_MS = 15_000;

export type ConfirmedTwapHandle = {
  readonly taskId: string;
  readonly report: () => MarketTwapReport;
  readonly outcomeDeadlineAtMs: number;
  /** Resolves when one-shot execution can finalize, including a latched incident. */
  readonly outcome: Promise<MarketTwapReport>;
  /** Resolves only after the hosted task is fenced and removed. */
  readonly settled: Promise<MarketTwapReport>;
};

export type ConfirmedTwapHost = {
  start(spec: MarketTwapSpec): Promise<ConfirmedTwapHandle>;
  list(): readonly MarketTwapReport[];
  stop(taskId: string): Promise<MarketTwapReport>;
  stopAll(): Promise<readonly MarketTwapReport[]>;
  quiesce(deadlineAtMs: number): Promise<readonly MarketTwapReport[]>;
  hasLiveWork(): boolean;
};

type LiveTask = {
  readonly task: MarketTwapTask;
  readonly handle: ConfirmedTwapHandle;
  readonly publishOutcome: (report: MarketTwapReport) => void;
  readonly settle: (report: MarketTwapReport) => void;
  outcomePublished: boolean;
  settled: boolean;
};

export const createConfirmedTwapHost = (opts: {
  readonly accountName: string;
  readonly exchange: Exchange;
  readonly signerFingerprint: string;
  readonly audit?: ConfirmedAuditSink;
  readonly nowMs?: () => number;
  readonly onTaskSettled?: (report: MarketTwapReport) => void;
}): ConfirmedTwapHost => {
  const orders = createConfirmedOrders({
    exchange: opts.exchange,
    signerFingerprint: opts.signerFingerprint,
    audit: opts.audit,
  });
  const tasks = new Map<string, LiveTask>();
  const nowMs = opts.nowMs ?? (() => Date.now());

  const syncSettled = (): void => {
    for (const [taskId, live] of tasks) {
      if (live.settled) continue;
      const isSettled = live.task.isSettled();
      const report = live.task.report();
      const hasIncident = report.slices.some((slice) => slice.truth?.kind === 'incident');
      if (!live.outcomePublished && (isSettled || hasIncident)) {
        live.outcomePublished = true;
        live.publishOutcome(report);
      }
      if (!isSettled) continue;
      live.settled = true;
      tasks.delete(taskId);
      opts.onTaskSettled?.(report);
      live.settle(report);
    }
  };
  const runner = new ConfirmedTwapRunner({ nowMs, onChange: syncSettled });

  return {
    async start(spec) {
      if (tasks.has(spec.taskId)) {
        throw new Error(`Confirmed TWAP ${spec.taskId} already exists on ${opts.accountName}`);
      }
      const task = new MarketTwapTask({ spec, orders });
      const outcome = Promise.withResolvers<MarketTwapReport>();
      const settled = Promise.withResolvers<MarketTwapReport>();
      const handle: ConfirmedTwapHandle = {
        taskId: spec.taskId,
        report: () => task.report(),
        outcomeDeadlineAtMs: spec.startAtMs + spec.durationMs + CONFIRMED_OUTCOME_GRACE_MS,
        outcome: outcome.promise,
        settled: settled.promise,
      };
      tasks.set(spec.taskId, {
        task,
        handle,
        publishOutcome: outcome.resolve,
        settle: settled.resolve,
        outcomePublished: false,
        settled: false,
      });
      task.start();
      runner.add(task);
      syncSettled();
      return handle;
    },

    list() {
      return [...tasks.values()].map((live) => live.task.report());
    },

    async stop(taskId) {
      const live = tasks.get(taskId);
      if (live === undefined) throw new Error(`No confirmed TWAP ${taskId} on ${opts.accountName}`);
      live.task.stop();
      syncSettled();
      await runner.waitUntil(live.task, nowMs() + 15_000);
      syncSettled();
      return live.task.report();
    },

    async stopAll() {
      const deadlineAtMs = nowMs() + 15_000;
      const stopping = [...tasks.values()];
      for (const live of stopping) live.task.stop();
      syncSettled();
      await Promise.all(stopping.map((live) => runner.waitUntil(live.task, deadlineAtMs)));
      const reports = stopping.map((live) => live.task.report());
      syncSettled();
      return reports;
    },

    async quiesce(deadlineAtMs) {
      const reports = await runner.quiesce(deadlineAtMs);
      syncSettled();
      return reports;
    },

    hasLiveWork() {
      return runner.hasLiveWork();
    },
  };
};
