import type { ExecuteResult } from '../../../packages/cli-core/src/commands/CommandExecutor';

import { deserializeCommandError, serializeCommandError } from '../../../packages/safe-cex/src/commandOutcome';

export const commandResultError = (result: Pick<ExecuteResult, 'error' | 'errorDetails' | 'errorReported'>) => {
  const error = deserializeCommandError(result.errorDetails ?? result.error ?? 'Command failed') as Error & {
    alreadyReported?: boolean;
  };
  if (result.error !== undefined) error.message = result.error;
  if (result.errorReported) error.alreadyReported = true;
  return error;
};

/** Never repeat a write which may already exist or has known accepted members. */
export const runCommandWithRetries = async (run: () => Promise<void>, retries: number): Promise<void> => {
  if (!Number.isInteger(retries) || retries < 0 || retries > 10) throw new RangeError('Retry count must be from 0 to 10');
  for (let attempt = 0; ; attempt++) {
    try {
      await run();
      return;
    } catch (failure) {
      const details = serializeCommandError(failure);
      if (details.outcome === 'unknown' || details.partialOrderIds?.length || details.partialOrderCount) throw failure;
      if (attempt >= retries) {
        const error = deserializeCommandError(details) as Error & { alreadyReported?: boolean };
        error.message = `(after ${retries} retries) ${error.message}`;
        if (failure instanceof Error && 'alreadyReported' in failure && failure.alreadyReported === true)
          error.alreadyReported = true;
        throw error;
      }
    }
  }
};
