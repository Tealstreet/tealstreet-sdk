import type { ExchangeName, Market, OrderBook, Ticker } from '@tealstreet/safe-cex/src/types';
import type {
  CoreAdapter,
  HostAccount,
  HostBindings,
  HostExchange,
  PrintKind,
  RunCommandResult,
} from '@tealstreet/script-api-core';

import { executeCommand, type CLIPlatformAdapter, type CLISession, type ExchangeLike } from '@tealstreet/cli-core';

import { createCoreAdapter } from '@tealstreet/script-api-core';

import type { ExchangeManager } from './exchange-manager';
import { recordScriptWrite } from './audit-log';
import { getScriptOrderbookCache } from './script-orderbook-cache';

/**
 * The CLI SPOKE — bindings, not an adapter.
 *
 * This was a 485-line hand-rolled `CoreAdapter`, one of three implementations
 * of the same interface, and the drift was real and one-directional: it
 * applied **no rounding and no minimum-size validation** while web applied
 * both, on the same contract. The same script placed different numbers
 * depending on which runtime ran it.
 *
 * The body is now `@tealstreet/script-api-core`'s `createCoreAdapter`, so the
 * CLI gets the rounding, the venue minimums, the infinity-stop carve-out and
 * the reduce-only clamp it never had. What remains here is what only the CLI
 * can answer.
 *
 * **Do not reintroduce a method here.** A method on this side is how three
 * adapters happened in the first place; if something cannot be expressed as a
 * binding, add a binding.
 */

const storeOf = (exchange: ExchangeLike) => exchange.store ?? {};

/** Map `PrintKind` onto the existing `CLIPlatformAdapter.printX` family. */
const routePrint = (adapter: CLIPlatformAdapter, text: string, kind: PrintKind): void => {
  switch (kind) {
    case 'success':
      adapter.printSuccess(text);
      return;
    case 'info':
      adapter.printInfo(text);
      return;
    case 'warn':
      adapter.printWarning(text);
      return;
    case 'error':
      adapter.printError(text);
      return;
    case 'print':
    default:
      adapter.print([text]);
      return;
  }
};

export const createStandaloneHostBindings = (
  exchangeManager: ExchangeManager,
  session: CLISession,
  platformAdapter?: CLIPlatformAdapter,
): HostBindings => {
  const orderbookCache = getScriptOrderbookCache(exchangeManager);

  const requireAdapter = (): CLIPlatformAdapter => {
    if (!platformAdapter) {
      throw new Error(
        'standalone CoreAdapter built without CLIPlatformAdapter — pass one before invoking print/runCommand',
      );
    }
    return platformAdapter;
  };

  return {
    getExchange: (account): HostExchange | undefined =>
      exchangeManager.getExchange(account) as unknown as HostExchange | undefined,
    getStore: (account) => {
      const exchange = exchangeManager.getExchange(account);
      return exchange ? (storeOf(exchange) as never) : undefined;
    },

    readActiveAccount: () => session.currentAccountName ?? '',
    readActiveSymbol: () => session.currentSymbol,
    readStreamingAccounts: () => exchangeManager.getConnectedAccountNames(),
    readAllAccounts: (): readonly HostAccount[] =>
      exchangeManager.getAll().map((running) => ({
        name: running.accountName,
        exchange: running.accountConfig.exchange.toLowerCase() as ExchangeName,
      })),
    readExchangeNameForAccount: (account): ExchangeName | undefined => {
      const running = account ? exchangeManager.get(account) : undefined;
      return running?.accountConfig?.exchange?.toLowerCase() as ExchangeName | undefined;
    },

    /** Subscription-backed cache around safe-cex's `listenOrderBook`. */
    readOrderbook: (symbol, opts): Promise<OrderBook | undefined> => orderbookCache.get(symbol, opts),
    // `readHistoricalPosition` is deliberately absent: the CLI carries no
    // position history, which is why `getHistoricalPosition` stays out of the
    // CLI target surface.

    findMarket: (symbol, account): Market | undefined => {
      const exchange = exchangeManager.getExchange(account);
      return exchange ? storeOf(exchange).markets?.find((market) => market.symbol === symbol) : undefined;
    },
    findTicker: (symbol, account): Ticker | undefined => {
      const exchange = exchangeManager.getExchange(account);
      return exchange ? storeOf(exchange).tickers?.find((ticker) => ticker.symbol === symbol) : undefined;
    },

    /** The audit log. It fires from inside the body's own catch, so a
     *  `closePosition` that throws between legs still records the ids that DID
     *  go live — `rememberOrderLineage` needs them. */
    onWrite: ({ account, summary, ok, orderIds, error }) => {
      recordScriptWrite(account, summary, {
        ok,
        ...(orderIds ? { orderIds: [...orderIds] } : {}),
        ...(error ? { error } : {}),
      });
    },

    print: (text, kind) => {
      routePrint(requireAdapter(), text, kind);
    },
    runCommand: async (command, { account, symbol }): Promise<RunCommandResult> => {
      const platform = requireAdapter();
      const exchange = exchangeManager.getExchange(account) ?? null;
      const resolvedSymbol = symbol ?? session.currentSymbol;
      try {
        const result = await executeCommand(command, resolvedSymbol, account, exchange, platform);
        if (!result.handled) {
          // CLI-specific and documented in `apps/cli/CLAUDE.md`: an unhandled
          // verb is named back rather than reported as a generic failure.
          const verb = command.trim().split(/\s+/)[0] ?? '';
          return {
            ok: false,
            capturedIds: [],
            output: [],
            error: verb ? `Unknown command: ${verb}` : 'Unknown command',
          };
        }
        const success = result.success !== false;
        return {
          ok: success,
          capturedIds: result.data?.orderIds ?? [],
          // `executeCommand` prints through the platform adapter directly; the
          // sink is not returned here, so scripts see the transcript via
          // `api.io.print` rather than bundled into this result.
          output: [],
          error: success ? undefined : result.error,
        };
      } catch (error) {
        return {
          ok: false,
          capturedIds: [],
          output: [],
          error: error instanceof Error ? error.message : String(error),
        };
      }
    },
  };
};

/**
 * Build a `CoreAdapter` for the standalone CLI runtime.
 *
 * `platformAdapter` is optional only because some call sites do not carry one;
 * `apps/cli/src/script-runner.ts` always passes it. When absent, `print` and
 * `runCommand` throw so a wiring gap surfaces instead of silently no-oping.
 */
export const createStandaloneCoreAdapter = (
  exchangeManager: ExchangeManager,
  session: CLISession,
  platformAdapter?: CLIPlatformAdapter,
): CoreAdapter => createCoreAdapter(createStandaloneHostBindings(exchangeManager, session, platformAdapter));
