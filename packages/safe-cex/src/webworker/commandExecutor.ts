import type { Exchange } from '../exchanges/base';

/**
 * Runs CLI trading commands against a live exchange inside the worker.
 *
 * safe-cex deliberately does NOT implement this — the CLI command layer
 * (cli-core) does. The composition root (the web app) injects the
 * cli-core-backed implementation via {@link registerCommandExecutor} at worker
 * startup, so safe-cex carries no dependency on the CLI layer. Both the web
 * worker and the exchange-server register one; a host that does not answers
 * `exec` with a failed result.
 */
export type CommandExecutor = (
  exchange: Exchange,
  symbol: string,
  commandsString: string,
  reqId?: string,
) => Promise<void>;

let registeredExecutor: CommandExecutor | null = null;

/** Register the command executor (called once by the composition root). */
export const registerCommandExecutor = (executor: CommandExecutor): void => {
  registeredExecutor = executor;
};

/** The registered executor, or `null` if no host wired one up. */
export const getCommandExecutor = (): CommandExecutor | null => registeredExecutor;

/** Clear the registered executor. Primarily for tests / HMR teardown. */
export const resetCommandExecutor = (): void => {
  registeredExecutor = null;
};
