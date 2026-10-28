import type { CLIPlatformAdapter, ExchangeLike } from '../../../packages/cli-core/src/types';
import type { ExchangeHealthMonitor } from '../../../packages/safe-cex/src/health/ExchangeHealthMonitor';
import { OrderSide, type OrderFillEvent } from '../../../packages/safe-cex/src/types';
import type { recordFill } from './audit-log';
import { classifyForCLI, criticalErrorKey } from './error-classifier';
import type { logger } from './logger';

export interface ExchangeListenerContext {
  adapter: Pick<CLIPlatformAdapter, 'printSuccess' | 'printError'>;
  logger: Pick<typeof logger, 'info'>;
  recordFill: typeof recordFill;
  isMonitorMode(): boolean;
  getCurrentAccountName(): string | undefined;
  getHealthMonitor(accountName: string): Pick<ExchangeHealthMonitor, 'processMessage'> | undefined;
  addWatchMessage(type: 'error' | 'warning' | 'info' | 'fill', message: string): void;
}

// Track critical errors that have already been shown (account:category)
const shownCriticalErrors = new Set<string>();

/** Format a fill event for display. */
function formatFill(fill: OrderFillEvent): string {
  const side = fill.side === OrderSide.Buy ? 'Bought' : 'Sold';
  const priceStr = fill.price?.toLocaleString() ?? 'market';
  return `${side} ${fill.amount} ${fill.symbol} @ $${priceStr}`;
}

// `use <account>` on an account that is already enabled hands back the same
// exchange; wiring it again printed every fill once per switch.
const wiredEmitters = new WeakSet<object>();

/**
 * Setup event listeners on an exchange, once per exchange
 */
export function setupExchangeListeners(
  exchange: Pick<ExchangeLike, 'emitter'>,
  context: ExchangeListenerContext,
  accountName?: string,
): void {
  if (wiredEmitters.has(exchange.emitter)) return;
  wiredEmitters.add(exchange.emitter);

  const { adapter, logger, recordFill, addWatchMessage } = context;

  // Listen for command output. ExchangeLike.emitter is a generic
  // `(...args: unknown[]) => void` shape, so per-event payload types are
  // narrowed locally at each handler entry.
  exchange.emitter.on('exec-log', (...args: unknown[]) => {
    const { message } = args[0] as { message: string };
    if (context.isMonitorMode()) {
      // Buffer messages during watch mode so they don't get wiped
      addWatchMessage('info', message);
    } else {
      console.log(message);
    }
  });

  // Listen for fills
  exchange.emitter.on('fill', (...args: unknown[]) => {
    const fill = args[0] as OrderFillEvent;
    const fillMsg = formatFill(fill);
    logger.info(`Fill: ${fillMsg}`, fill);
    // TSBUG-107 slice 4: attribute the fill back to the run/command that
    // placed the order (no-op when auditing is off or the order isn't
    // tracked). Account comes from the remembered lineage. Independent of
    // display mode.
    recordFill(fill.orderId ?? fill.id, fillMsg);
    if (context.isMonitorMode()) {
      addWatchMessage('fill', fillMsg);
    } else {
      adapter.printSuccess(`✓ Fill: ${fillMsg}`);
    }
  });

  // Listen for errors — classify via health monitor before displaying
  exchange.emitter.on('error', (...args: unknown[]) => {
    const error = args[0] as string;
    const acctName = accountName || context.getCurrentAccountName() || 'unknown';
    const monitor = context.getHealthMonitor(acctName);
    const category = monitor ? monitor.processMessage(error, true) : null;
    const action = classifyForCLI(category, 'error', error);

    switch (action.action) {
      case 'suppress':
        // Don't print at all
        return;
      case 'critical': {
        const key = criticalErrorKey(acctName, category!);
        if (shownCriticalErrors.has(key)) return; // Already shown
        shownCriticalErrors.add(key);
        const msg = action.message || error;
        if (context.isMonitorMode()) {
          addWatchMessage('error', `[${category!.toUpperCase()}] ${acctName}: ${msg}`);
        } else {
          adapter.printError(
            `[${category!.toUpperCase()}] Account "${acctName}": ${msg}. Run "accounts disable ${acctName}" to stop.`,
          );
        }
        return;
      }
      case 'warn':
        if (context.isMonitorMode()) {
          addWatchMessage('warning', error);
        } else {
          console.warn(`Exchange warning: ${error}`);
        }
        return;
      case 'error': {
        const errMsg = action.message ? `${action.message}: ${error}` : error;
        if (context.isMonitorMode()) {
          addWatchMessage('error', errMsg);
        } else {
          console.error(`Exchange error: ${errMsg}`);
        }
        return;
      }
      case 'passthrough':
      default:
        if (context.isMonitorMode()) {
          addWatchMessage('error', error);
        } else {
          console.error(`Exchange error: ${error}`);
        }
        return;
    }
  });

  // Listen for warnings — classify via health monitor before displaying
  exchange.emitter.on('warning', (...args: unknown[]) => {
    const warning = args[0] as string;
    const acctName = accountName || context.getCurrentAccountName() || 'unknown';
    const monitor = context.getHealthMonitor(acctName);
    const category = monitor ? monitor.processMessage(warning, false) : null;
    const action = classifyForCLI(category, 'warning', warning);

    if (action.action === 'suppress') return;

    if (context.isMonitorMode()) {
      addWatchMessage('warning', warning);
    } else {
      console.warn(`Exchange warning: ${warning}`);
    }
  });
}
