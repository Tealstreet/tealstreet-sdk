import { createHash, randomBytes } from 'crypto';

import type { Exchange } from '@tealstreet/safe-cex/src/exchanges/base';
import type { Market, OrderSide } from '@tealstreet/safe-cex/src/types';

import { appendAuditEntry, isAuditEnabled } from '../../../audit-log';
import { SESSION_BOUND_BANNER, admitTwapEngine } from './admit';
import { createConfirmedTwapHost, type ConfirmedTwapHandle, type ConfirmedTwapHost } from './ConfirmedTwapHost';
import type { MarketTwapReport } from './MarketTwapTask';
import { contractsToBaseQuantity, exactFromFiniteNumber } from './contractQuantity';
import {
  deriveTwapQuantityConstraints,
  isTwapQuantizeRejection,
  quantizeTwapTarget,
  twapMetadataFromMarket,
} from './quantize';

export const confirmedTwapHosts = new Map<string, ConfirmedTwapHost>();
export const confirmedTwapHistory = new Map<string, MarketTwapReport>();

export type StartedConfirmedTwap = {
  readonly sequence: number;
  readonly accountName: string;
  readonly handle: ConfirmedTwapHandle;
};

const startedConfirmedTwaps: StartedConfirmedTwap[] = [];
let nextStartSequence = 0;
let admissionsFenced = false;

export const confirmedStartCursor = (): number => nextStartSequence;

export const confirmedStartsSince = (cursor: number): readonly StartedConfirmedTwap[] =>
  startedConfirmedTwaps.filter((started) => started.sequence >= cursor);

export const confirmedAdmissionsAreFenced = (): boolean => admissionsFenced;

/**
 * Synchronously fences new confirmed starts and future slices, then waits for
 * bounded-rate truth observation only until the caller's absolute deadline.
 */
export const quiesceConfirmedTwaps = (deadlineAtMs: number): Promise<readonly MarketTwapReport[]> => {
  admissionsFenced = true;
  const pending = [...confirmedTwapHosts.values()].map((host) => host.quiesce(deadlineAtMs));
  return Promise.all(pending).then((reports) => reports.flat());
};

export const startConfirmedTwap = async (input: {
  readonly accountName: string;
  readonly exchange: Exchange;
  readonly market: Market;
  readonly side: OrderSide;
  readonly totalSize: number;
  readonly sizeIsNative?: boolean;
  readonly slices: number;
  readonly durationMs: number;
  readonly reduceOnly?: boolean;
  readonly explicitLegacy: boolean;
}): Promise<
  | { readonly engine: 'legacy' }
  | {
      readonly engine: 'confirmed';
      readonly banner: string;
      readonly taskId: string;
      readonly outcomeDeadlineAtMs: number;
      readonly settled: Promise<MarketTwapReport>;
      readonly outcome: Promise<MarketTwapReport>;
    }
  | { readonly engine: 'error'; readonly message: string }
> => {
  const admission = admitTwapEngine({
    exchange: input.exchange,
    market: input.market,
    explicitLegacy: input.explicitLegacy,
  });
  if (admission.engine !== 'confirmed') return admission;
  if (admissionsFenced) {
    return { engine: 'error', message: 'Confirmed TWAP shutdown is in progress; no new task can start' };
  }

  const ticker = input.exchange.store.tickersBySymbol[input.market.symbol];
  const metadata = twapMetadataFromMarket(input.market, ticker?.mark ?? ticker?.last);
  const constraints = deriveTwapQuantityConstraints(metadata);
  if (isTwapQuantizeRejection(constraints)) {
    return { engine: 'error', message: constraints.reason };
  }
  const contractSize = constraints.contractSize;
  const baseTotal =
    input.sizeIsNative === true
      ? Number(
          contractsToBaseQuantity(
            exactFromFiniteNumber(input.totalSize) ?? {
              decimal: String(input.totalSize),
              atoms: String(Math.round(input.totalSize)),
              atomScale: 0,
            },
            contractSize,
          ).decimal,
        )
      : input.totalSize;
  const quantized = quantizeTwapTarget({
    metadata,
    target: { kind: 'base', quantity: baseTotal },
    sliceCount: input.slices,
  });
  if (quantized.kind === 'rejected') return { engine: 'error', message: quantized.reason };
  if (admissionsFenced) {
    return { engine: 'error', message: 'Confirmed TWAP shutdown is in progress; no new task can start' };
  }

  let host = confirmedTwapHosts.get(input.accountName);
  if (host === undefined) {
    let createdHost: ConfirmedTwapHost;
    createdHost = createConfirmedTwapHost({
      accountName: input.accountName,
      exchange: input.exchange,
      signerFingerprint: createHash('sha256').update(input.exchange.options.key).digest('hex').slice(0, 16),
      audit: {
        emit(event) {
          if (!isAuditEnabled()) return;
          appendAuditEntry({
            ts: new Date().toISOString(),
            account: input.accountName,
            command: `[confirmed] ${event.kind} ${JSON.stringify(event)}`,
            ok: true,
          });
        },
      },
      onTaskSettled(report) {
        confirmedTwapHistory.set(report.taskId, report);
        if (confirmedTwapHosts.get(input.accountName) === createdHost && createdHost.list().length === 0) {
          confirmedTwapHosts.delete(input.accountName);
        }
      },
    });
    host = createdHost;
    confirmedTwapHosts.set(input.accountName, host);
  }

  const taskId = `ctwap-${Date.now().toString(36)}-${randomBytes(3).toString('hex')}`;
  const handle = await host.start({
    taskId,
    market: input.market,
    side: input.side,
    reduceOnly: input.reduceOnly,
    target: quantized.target,
    quantityStep: quantized.quantityStep,
    minQuantity: quantized.minQuantity,
    maxQuantity: quantized.maxQuantity,
    contractSize: quantized.contractSize,
    sliceCount: input.slices,
    startAtMs: Date.now(),
    durationMs: input.durationMs,
  });
  startedConfirmedTwaps.push({ sequence: nextStartSequence, accountName: input.accountName, handle });
  nextStartSequence += 1;
  return {
    engine: 'confirmed',
    banner: SESSION_BOUND_BANNER,
    taskId,
    outcomeDeadlineAtMs: handle.outcomeDeadlineAtMs,
    outcome: handle.outcome,
    settled: handle.settled,
  };
};
