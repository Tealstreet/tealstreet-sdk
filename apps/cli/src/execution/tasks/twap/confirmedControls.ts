import { confirmedTwapHistory, confirmedTwapHosts } from './startConfirmed';
import type { MarketTwapReport } from './MarketTwapTask';

export const FLATTEN_WARNING = (ids: string, symbol: string): string =>
  `WARNING: confirmed TWAP ${ids} is still active for ${symbol}; later slices may reopen the position. Stop or kill it first if this flatten must remain flat.`;

export const ACCOUNT_DISABLE_ERROR = (account: string): string =>
  `Confirmed TWAPs are active on ${account}; stop them or run kill before disabling this account.`;

export const listConfirmedReports = (): MarketTwapReport[] => {
  const reports = new Map(confirmedTwapHistory);
  for (const host of confirmedTwapHosts.values()) {
    for (const report of host.list()) reports.set(report.taskId, report);
  }
  return [...reports.values()];
};

export const formatConfirmedList = (): string[] =>
  listConfirmedReports().map((report) => {
    const side = report.side.toUpperCase();
    return `  CONFIRMED [${report.taskId}] ${side} ${report.symbol} ${report.lifecycle} filled=${report.confirmedFilled.decimal} reserved=${report.reservedUnresolved.decimal}`;
  });
export const formatConfirmedFinalReport = (report: MarketTwapReport): string[] => {
  const lines: string[] = [];
  for (const slice of report.slices) {
    const truth = slice.truth;
    const handle = truth?.handle;
    const lastGood = truth?.kind === 'incident' ? truth.lastGood : truth;
    const observation =
      lastGood?.kind === 'open' || lastGood?.kind === 'terminal'
        ? lastGood.observation
        : undefined;
    const status =
      truth?.kind === 'incident'
        ? `incident:${truth.reason}`
        : observation?.status ?? truth?.kind ?? slice.invalidReason ?? 'not_started';
    lines.push(
      [
        `CONFIRMED task=${report.taskId}`,
        `owner=${handle?.identity.ownerId ?? report.taskId}`,
        `unit=${slice.unitId}`,
        `caller=${handle?.callerId ?? '-'}`,
        `wire=${handle?.wireClientOrderId ?? '-'}`,
        `venue=${observation?.venueOrderId ?? handle?.venueOrderId ?? '-'}`,
        `status=${status}`,
        `total=${observation?.total.decimal ?? handle?.exact.decimal ?? slice.requested.decimal}`,
        `fill=${observation?.filled.decimal ?? '0'}`,
      ].join(' '),
    );
  }
  lines.push(
    `TOTAL task=${report.taskId} confirmed=${report.confirmedFilled.decimal} rejected=${report.rejected.decimal} residual=${report.residual.decimal} reserved=${report.reservedUnresolved.decimal}`,
  );
  return lines;
};

export const tryStopConfirmed = async (id: string): Promise<MarketTwapReport | undefined> => {
  for (const host of confirmedTwapHosts.values()) {
    const match = host.list().find((report) => report.taskId === id || report.taskId.startsWith(id));
    if (match !== undefined) {
      return host.stop(match.taskId);
    }
  }
  return undefined;
};

export const killAllConfirmed = async (): Promise<number> => {
  const reportsByHost = await Promise.all(
    [...confirmedTwapHosts.values()].map((host) => host.stopAll()),
  );
  return reportsByHost.reduce((count, reports) => count + reports.length, 0);
};

export const activeConfirmedForSymbol = (symbol: string): string[] =>
  listConfirmedReports()
    .filter((report) => report.symbol === symbol && (report.lifecycle === 'running' || report.lifecycle === 'paused' || report.lifecycle === 'stopping'))
    .map((report) => report.taskId);

export const hostHasLiveWork = (accountName: string): boolean =>
  confirmedTwapHosts.get(accountName)?.hasLiveWork() === true;

export const oneShotExitCode = (reports: readonly MarketTwapReport[]): number => {
  if (reports.some((report) => report.reservedUnresolved.atoms !== '0' || report.slices.some((slice) => slice.truth?.kind === 'incident' || slice.truth?.kind === 'unconfirmed'))) {
    return 3;
  }
  if (reports.some((report) => report.lifecycle === 'partial' || report.lifecycle === 'stopped' || report.rejected.atoms !== '0')) {
    return 2;
  }
  if (reports.every((report) => report.lifecycle === 'completed')) {
    return 0;
  }
  return 1;
};
