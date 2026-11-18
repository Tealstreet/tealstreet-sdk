import type { ChaserCompleteEvent } from '@tealstreet/safe-cex/src/webworker/chaserOrder/ChaserOrderOptions';
import { getActiveChasers } from '../../../packages/cli-core/src/commands/trading/ChaseCommand';
import { getStorageEpoch, isStorageSessionCurrent } from './storage-context';

/** A chase lives in this process, which exits now: its order stays with nothing managing it. */
export function chaseAbandonedByExit(): string | undefined {
  if (getActiveChasers().length === 0) return undefined;
  return `A chase runs only while this process does, and this one-shot command is exiting: nothing is chasing now and its limit order may still be open. Check open orders, and chase from the interactive prompt.`;
}

// Covers a venue's post-only retries and the lost-placement lookups (15 s).
const FIRST_PLACEMENT_WAIT_MS = 20_000;
const FIRST_PLACEMENT_POLL_MS = 50;

export type OneShotChaseProblem = { code: 'COMMAND_FAILED' | 'CHASE_ABANDONED'; error: string };

/**
 * Start before the command. The returned function waits for each chase's first
 * placement, then says what the exit means: a chase that placed nothing failed.
 */
export function watchOneShotChases(exchange: {
  emitter: {
    on: (event: string, handler: (...args: unknown[]) => void) => void;
    off: (event: string, handler: (...args: unknown[]) => void) => void;
  };
}): () => Promise<OneShotChaseProblem | undefined> {
  const ownerEpoch = getStorageEpoch();
  const completed: ChaserCompleteEvent[] = [];
  const onComplete = (event: unknown) => {
    if (isStorageSessionCurrent(ownerEpoch)) completed.push(event as ChaserCompleteEvent);
  };
  exchange.emitter.on('chaser:complete', onComplete);
  return async () => {
    const deadline = Date.now() + FIRST_PLACEMENT_WAIT_MS;
    while (
      isStorageSessionCurrent(ownerEpoch) &&
      getActiveChasers().some((chase) => chase.everTrackedOrderIds.size === 0) &&
      Date.now() < deadline
    ) {
      await new Promise((resolve) => setTimeout(resolve, FIRST_PLACEMENT_POLL_MS));
    }
    exchange.emitter.off('chaser:complete', onComplete);
    if (!isStorageSessionCurrent(ownerEpoch)) return undefined;
    const failed = completed.filter((chase) => !chase.placed && chase.filled === 0);
    if (failed.length > 0) {
      return {
        code: 'COMMAND_FAILED',
        error: failed.map((chase) => `Chase on ${chase.symbol} placed no order: ${chase.stopReason}`).join('; '),
      };
    }
    const abandoned = chaseAbandonedByExit();
    return abandoned ? { code: 'CHASE_ABANDONED', error: abandoned } : undefined;
  };
}
