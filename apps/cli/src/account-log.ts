import type { ExchangeAccountConfig } from '@tealstreet/cli-core';

import { logger } from './logger';

type AccountForLog = Partial<ExchangeAccountConfig> & {
  exchange?: string;
  name?: string;
};

export function summarizeAccountForLog(account: AccountForLog) {
  return {
    exchange: account.exchange,
    name: account.name,
    testnet: account.testnet,
    active: account.active,
    deletedAt: account.deletedAt,
    updatedAt: account.updatedAt,
    tealstreetProxy: account.tealstreetProxy,
    tealstreetTunnel: account.tealstreetTunnel,
    tealstreetWsRoute: account.tealstreetWsRoute,
    proxyRegion: account.proxyRegion,
    hasApiKey: Boolean(account.apiKey),
  };
}

export function summarizeAccountsForLog(accounts: AccountForLog[]) {
  const rows = accounts.map(summarizeAccountForLog);
  const byExchange: Record<string, number> = {};
  const byRegion: Record<string, number> = {};

  for (const row of rows) {
    const exchange = row.exchange ?? '<unknown>';
    const region = row.proxyRegion ?? '<missing>';
    byExchange[exchange] = (byExchange[exchange] ?? 0) + 1;
    byRegion[region] = (byRegion[region] ?? 0) + 1;
  }

  return {
    count: rows.length,
    byExchange,
    byRegion,
    rows: rows.slice(0, 50),
    truncated: rows.length > 50,
  };
}

export function logAccountRoute(message: string, account: AccountForLog, detail?: Record<string, unknown>): void {
  logger.info(message, {
    ...detail,
    account: summarizeAccountForLog(account),
  });
}
