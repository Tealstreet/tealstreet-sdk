import type { Exchange } from '@tealstreet/safe-cex/src/exchanges/base';
import { executeCommand, type ExecuteResult } from '../commands/CommandExecutor';
import type { ExchangeLike } from '../types';
import { WorkerPlatformAdapter } from './WorkerPlatformAdapter';

/**
 * Route a single trading command to the appropriate handler.
 * This processes one command at a time - use routeCommands() for semicolon-chained commands.
 *
 * Note: Session commands like 'account' and 'switch' should be handled by the
 * web app UI layer before reaching this router. The webworker only handles
 * trading commands that need exchange access.
 *
 * Returns the cli-core ExecuteResult so callers can post it back to the
 * browser (used by the capture-syntax writeback path — the CLI matches
 * `result.data.orderIds` against a pending `@<name>` write).
 */
export const commandRouter = async (
  exchange: Exchange,
  symbol: string,
  command: string,
  reqId?: string,
): Promise<ExecuteResult> => {
  // Create adapter for this exchange - routes output via exec-log events
  const adapter = new WorkerPlatformAdapter(exchange as unknown as ExchangeLike, reqId);

  // Execute command through cli-core
  const result = await executeCommand(
    command,
    symbol,
    undefined, // accountName - not used in webworker context
    exchange as unknown as ExchangeLike,
    adapter,
  );

  // If command wasn't handled by cli-core, it's unknown
  if (!result.handled) {
    exchange.emitter.emit('exec-log', {
      message: `Unknown command: ${command.split(' ')[0]}`,
      ...(reqId === undefined ? {} : { reqId }),
    });
  }
  return result;
};

/**
 * Route multiple commands separated by semicolons.
 * Supports @ prefix for parallel (fire-and-forget) execution.
 * Use this for user input that may contain chained commands.
 *
 * When `reqId` is provided, emits an `exec-result` event after the LAST
 * sequential command completes. The browser-side CLI listens for this
 * to fulfil the `@<name> = <body>` capture-syntax writeback. We only
 * carry the LAST result because preprocessed steps from the web CLI
 * are typically single commands (the preprocess already split on `;`
 * before forwarding to the worker) — the loop here is defensive
 * against users who type `;` in an alias body. Parallel `@cmd` slices
 * don't carry capture semantics and their results are intentionally
 * discarded.
 */
export const routeCommands = async (exchange: Exchange, symbol: string, commandsString: string, reqId?: string) => {
  const commands = commandsString
    .split(';')
    .map((cmd) => cmd.trim())
    .filter((cmd) => cmd.length > 0);

  let lastResult: ExecuteResult | undefined;
  for (const command of commands) {
    if (command.startsWith('@')) {
      // Parallel execution - fire and forget
      commandRouter(exchange, symbol, command.slice(1), reqId).catch((e) => {
        exchange.emitter.emit('exec-log', {
          message: `Error in parallel command: ${(e as Error).message}`,
          ...(reqId === undefined ? {} : { reqId }),
        });
      });
    } else {
      // Sequential execution - wait for completion
      lastResult = await commandRouter(exchange, symbol, command, reqId);
      if (reqId === undefined && lastResult.handled && lastResult.success === false && !lastResult.errorReported) {
        exchange.emitter.emit('exec-log', {
          message: lastResult.error ?? 'Command failed',
        });
      }
    }
  }

  // Reply with the last sequential result. Browser-side CLI matches
  // the reqId in its pendingResults Map to resolve the awaited
  // dispatchStep promise. Send only the fields the CLI consumes —
  // avoid serialising non-cloneable values (e.g. callbacks) over
  // postMessage.
  if (reqId !== undefined) {
    const payload: {
      reqId: string;
      handled: boolean;
      success?: boolean;
      error?: string;
      errorReported?: boolean;
      errorDetails?: ExecuteResult['errorDetails'];
      data?: ExecuteResult['data'];
    } = {
      reqId,
      handled: lastResult?.handled ?? false,
    };
    if (lastResult?.success !== undefined) payload.success = lastResult.success;
    if (lastResult?.error !== undefined) payload.error = lastResult.error;
    if (lastResult?.errorDetails !== undefined) payload.errorDetails = lastResult.errorDetails;
    if (lastResult?.errorReported !== undefined) payload.errorReported = lastResult.errorReported;
    if (lastResult?.data !== undefined) payload.data = lastResult.data;
    exchange.emitter.emit('exec-result', payload);
  }
};

/**
 * The command executor safe-cex's webworker registers via
 * `registerCommandExecutor`. Lives here in cli-core (not in safe-cex) so the
 * exchange library carries no dependency on the CLI command layer — the
 * composition root (web app) wires this in. Matches safe-cex's
 * `CommandExecutor` port shape structurally.
 */
export const workerCommandExecutor = routeCommands;
