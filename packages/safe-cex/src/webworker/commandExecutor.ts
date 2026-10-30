import type { Exchange } from '../exchanges/base';

/**
 * Runs CLI trading commands against a live exchange: cli-core's executor, which
 * a host registers. The worker no longer calls it (its `exec` and Telegram
 * `/cli` run in the engine's Rust algos); only tealstreet-next still registers it.
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
