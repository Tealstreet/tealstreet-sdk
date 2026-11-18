import type { NukeResult } from './types';

/**
 * The line telling the user which running algos a nuke stopped before it closed
 * anything, or undefined when it stopped none. Accepts any resolved nuke value:
 * TS accounts resolve undefined, and hosted results cross a JSON bridge.
 */
export const describeStoppedAlgos = (result: unknown): string | undefined => {
  const stopped = (result ?? {}) as Partial<NukeResult>;
  const chasers = Array.isArray(stopped.stoppedChasers) ? stopped.stoppedChasers : [];
  const twaps = Array.isArray(stopped.stoppedTwaps) ? stopped.stoppedTwaps : [];
  if (!chasers.length && !twaps.length) return undefined;
  const list = (ids: string[]) => (ids.length ? ` (${ids.join(', ')})` : '');
  return `Nuke stopped ${chasers.length} chaser(s)${list(chasers)} and ${twaps.length} TWAP(s)${list(twaps)} before closing`;
};

/** The nuke's warning for the user, such as orders it may not have reached, if any. */
export const nukeWarning = (result: unknown): string | undefined => {
  const warning = (result as Partial<NukeResult> | null | undefined)?.warning;
  return typeof warning === 'string' && warning ? warning : undefined;
};
