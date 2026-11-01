import { getActiveChasers } from '../../../packages/cli-core/src/commands/trading/ChaseCommand';

/** A chase lives in this process, which exits now: its order stays with nothing managing it. */
export function chaseAbandonedByExit(): string | undefined {
  if (getActiveChasers().length === 0) return undefined;
  return `A chase runs only while this process does, and this one-shot command is exiting: nothing is chasing now and its limit order may still be open. Check open orders, and chase from the interactive prompt.`;
}
