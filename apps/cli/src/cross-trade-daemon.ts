// Headless cross-trade engine driver for the CLI.
//
// Boots the accounts referenced by enabled cross-trade configs via the
// existing ExchangeManager, then runs the lifted CrossTradeManager
// in-process. No REPL, no scroll region, no footer — just a long-lived
// audit loop with a periodic status line.
//
// Chase execution mode is NOT supported by the CLI engine in v1; configs
// with `copier.execution === 'Chase'` are dropped at load time with a
// per-config warning so the user knows which configs were skipped.

import type { CLIPlatformAdapter, ExchangeLike } from '@tealstreet/cli-core';
import type { CrossTradeConfig, CrossTradeCopier } from '@tealstreet/safe-cex/src/crossTrade/types';
import type { Exchange } from '@tealstreet/safe-cex/src/exchanges/base';
import type { Market, PlaceOrderOpts } from '@tealstreet/safe-cex/src/types';

import chalk from 'chalk';

import { DEFAULT_PRICE_PRECISION } from '@tealstreet/utils/src/currency/currencyUtils';
import { CrossTradeManager } from '@tealstreet/safe-cex/src/crossTrade/runtime/CrossTradeManager';
import { OrderSide, OrderType } from '@tealstreet/safe-cex/src/types';
import { adjust } from '@tealstreet/safe-cex/src/utils/safe-math';

import { listAccounts } from './config';
import { createStandalonePlatformAdapter } from './platform-adapter';
import { ExchangeManager } from './exchange-manager';
import { getRunnableCrossTradeConfigs } from './cross-trade-store';
import { clearHeartbeat, HEARTBEAT_INTERVAL_MS, writeHeartbeat } from './cross-trade-heartbeat';
import { recordCrossTradeEvent } from './audit-log';
import { logger } from './logger';
import { BRAND } from './brand';

const STATUS_INTERVAL_MS = 30_000;

// Same slippage cap used in the webworker placeOrder helper. v1 keeps it
// hard-coded; expose via per-copier config if users need to tune it.
const SLIPPAGE_LIMIT = 0.009;

interface DaemonContext {
  adapter: CLIPlatformAdapter;
  exchangeManager: ExchangeManager;
  manager: CrossTradeManager;
  configs: CrossTradeConfig[];
  statusTimer: ReturnType<typeof setInterval> | null;
  heartbeatTimer: ReturnType<typeof setInterval> | null;
  startedAt: number;
  stopped: boolean;
}

let currentContext: DaemonContext | null = null;

/**
 * Returns true iff a cross-trade daemon is currently running in this
 * process. Read by the listener broadcast to flip `state:cross-trade`.
 */
export function isCrossTradeDaemonRunning(): boolean {
  return !!currentContext && !currentContext.stopped;
}

export function getCrossTradeDaemonStats(): { running: boolean; configCount: number } {
  if (!currentContext || currentContext.stopped) {
    return { running: false, configCount: 0 };
  }
  return { running: true, configCount: currentContext.configs.length };
}

/**
 * CLI-side placeOrder adapter. Mirrors webworker/CrossTrade/crossTradePlaceOrder
 * for the LimitSlippage + Market paths; Chase is intentionally unsupported.
 * Throws when called on a Chase copier so CrossTradeManager's failure-mute
 * loop kicks in cleanly.
 */
function crossTradeCliPlaceOrder(getExchange: (account: string) => ExchangeLike | undefined) {
  return async (
    copier: CrossTradeCopier,
    market: Market,
    referencePrice: number | undefined,
    account: string,
    side: OrderSide,
    amount: number,
    reduceOnly: boolean,
  ) => {
    if (copier.execution === 'Chase') {
      throw new Error(`cross-trade(cli): Chase execution mode is not supported on the CLI engine in v1`);
    }
    if (!amount) return;
    const exchange = getExchange(account);
    if (!exchange?.placeOrder) {
      throw new Error(`cross-trade(cli): exchange ${account} has no placeOrder`);
    }
    const symbol = market.symbol;
    if (copier.execution === 'LimitSlippage') {
      if (!referencePrice) return;
      const maxSlipPrice =
        side === OrderSide.Buy ? referencePrice * (1 + SLIPPAGE_LIMIT) : referencePrice * (1 - SLIPPAGE_LIMIT);
      const adjustedPrice = adjust(maxSlipPrice, market.precision.price || DEFAULT_PRICE_PRECISION);
      const order: PlaceOrderOpts = {
        symbol,
        side,
        type: OrderType.Limit,
        price: adjustedPrice,
        reduceOnly,
        amount,
      };
      return await exchange.placeOrder(order);
    }
    if (copier.execution === 'Market') {
      const order: PlaceOrderOpts = {
        symbol,
        side,
        type: OrderType.Market,
        reduceOnly,
        amount,
      };
      return await exchange.placeOrder(order);
    }
    throw new Error(`cross-trade(cli): unknown execution mode ${String(copier.execution)}`);
  };
}

export function collectAccountsToBoot(configs: CrossTradeConfig[]): Set<string> {
  const names = new Set<string>();
  for (const cfg of configs) {
    if (cfg.masterAccount) names.add(cfg.masterAccount);
    for (const copier of cfg.copiers) {
      if (copier?.account && copier.enabled) names.add(copier.account);
    }
  }
  return names;
}

export function dropChaseConfigs(configs: CrossTradeConfig[]): { kept: CrossTradeConfig[]; dropped: number } {
  let dropped = 0;
  const kept: CrossTradeConfig[] = [];
  for (const cfg of configs) {
    const copiers = cfg.copiers.filter((c) => c?.enabled && c.execution !== 'Chase');
    if (copiers.length === cfg.copiers.filter((c) => c?.enabled).length) {
      kept.push(cfg);
    } else if (copiers.length > 0) {
      // Some copiers OK, drop the chase ones in place.
      kept.push({ ...cfg, copiers });
      dropped += cfg.copiers.filter((c) => c?.enabled && c.execution === 'Chase').length;
    } else {
      dropped += cfg.copiers.filter((c) => c?.enabled && c.execution === 'Chase').length;
    }
  }
  return { kept, dropped };
}

export interface RunCrossTradeDaemonOpts {
  /**
   * Override the boot list. When omitted, reads getRunnableCrossTradeConfigs().
   * Tests inject this so they don't have to mock the file IO.
   */
  configs?: CrossTradeConfig[];
}

/**
 * Foreground entry point. Returns when SIGINT / SIGTERM fires AND the engine
 * has finished its current loop iteration.
 */
export async function runCrossTradeDaemon(opts: RunCrossTradeDaemonOpts = {}): Promise<void> {
  if (currentContext) {
    throw new Error('cross-trade daemon already running in this process');
  }
  const adapter = createStandalonePlatformAdapter();
  const exchangeManager = new ExchangeManager(adapter);
  adapter.setExchangeManager(exchangeManager);

  const rawConfigs = opts.configs ?? getRunnableCrossTradeConfigs();
  if (rawConfigs.length === 0) {
    console.log(chalk.yellow('No runnable cross-trade configs.'));
    console.log(chalk.gray(`Add or enable one with: ${BRAND.binName} cross-trade add / enable`));
    return;
  }

  const { kept: configs, dropped } = dropChaseConfigs(rawConfigs);
  if (dropped > 0) {
    console.log(
      chalk.yellow(
        `\nSkipping ${dropped} copier(s) using Chase execution mode — not supported by the CLI engine in v1.\nUse Market or LimitSlippage to run those on the CLI; the web side still supports Chase.`,
      ),
    );
  }
  if (configs.length === 0) {
    console.log(chalk.red('All runnable configs use unsupported Chase mode. Nothing to start.'));
    return;
  }

  const accountsToBoot = collectAccountsToBoot(configs);
  const liveAccounts = new Map(listAccounts().map((a) => [a.name, a]));

  console.log(chalk.cyan(`\nStarting cross-trade engine for ${configs.length} config(s):`));
  for (const cfg of configs) {
    console.log(
      chalk.gray(
        `  ${cfg.masterAccount} ${cfg.masterSymbol ? `(${cfg.masterSymbol})` : '(any symbol)'} → ` +
          cfg.copiers
            .filter((c) => c?.enabled)
            .map((c) => `${c.account} [${c.execution}]`)
            .join(', '),
      ),
    );
  }

  for (const accountName of accountsToBoot) {
    const account = liveAccounts.get(accountName);
    if (!account) {
      console.log(chalk.red(`  ✗ Account "${accountName}" not found in config — skipped`));
      continue;
    }
    try {
      console.log(chalk.gray(`  Booting ${accountName} (${account.exchange})…`));
      await exchangeManager.enable(account);
    } catch (err) {
      console.log(chalk.red(`  ✗ Failed to boot ${accountName}: ${(err as Error).message}`));
    }
  }

  const placeOrder = crossTradeCliPlaceOrder((account) => exchangeManager.getExchange(account));
  const manager = new CrossTradeManager({
    getExchange: (account) => exchangeManager.getExchange(account) as Exchange | undefined,
    placeOrder,
  });

  const startedAt = Date.now();
  const ctx: DaemonContext = {
    adapter,
    exchangeManager,
    manager,
    configs,
    statusTimer: null,
    heartbeatTimer: null,
    startedAt,
    stopped: false,
  };
  currentContext = ctx;

  manager.start(configs);
  recordCrossTradeEvent('start', { ok: true });
  // Initial heartbeat fires synchronously so a connected REPL listener
  // sees the running state before the daemon yields back to the loop.
  writeHeartbeat({ startedAt, configCount: configs.length });
  ctx.heartbeatTimer = setInterval(() => {
    if (ctx.stopped) return;
    try {
      writeHeartbeat({ startedAt, configCount: configs.length });
    } catch (err) {
      logger.warn(`cross-trade heartbeat write failed: ${(err as Error).message}`);
    }
  }, HEARTBEAT_INTERVAL_MS);

  console.log(chalk.green('\n✓ Cross-trade engine running. Press Ctrl+C to stop.\n'));

  ctx.statusTimer = setInterval(() => {
    if (ctx.stopped) return;
    console.log(
      chalk.gray(
        `[cross-trade] ${configs.length} active config(s), ` +
          `${exchangeManager.getConnectedAccountNames().length} exchange(s) connected — ${new Date().toISOString()}`,
      ),
    );
  }, STATUS_INTERVAL_MS);

  await new Promise<void>((resolve) => {
    const shutdown = async (signal: string) => {
      if (ctx.stopped) return;
      ctx.stopped = true;
      console.log(chalk.yellow(`\n${signal} received — stopping cross-trade engine…`));
      recordCrossTradeEvent('stop', { ok: true });
      try {
        manager.stop();
        if (ctx.statusTimer) {
          clearInterval(ctx.statusTimer);
          ctx.statusTimer = null;
        }
        if (ctx.heartbeatTimer) {
          clearInterval(ctx.heartbeatTimer);
          ctx.heartbeatTimer = null;
        }
        clearHeartbeat();
        await exchangeManager.stopAll();
      } catch (err) {
        logger.warn(`cross-trade daemon shutdown error: ${(err as Error).message}`);
      } finally {
        currentContext = null;
        console.log(chalk.green('✓ Cross-trade engine stopped.\n'));
        resolve();
      }
    };

    process.once('SIGINT', () => void shutdown('SIGINT'));
    process.once('SIGTERM', () => void shutdown('SIGTERM'));
  });
}
