import type { Exchange } from '../exchanges/base';
import type { NukeResult } from '../types';

import { chaserOrderManagerInstance } from './chaserOrder/ChaserOrderManager';
import { twapManager } from './twap/TwapManager';

/**
 * A nuke's first step on `exchange`'s account: running TWAPs and chasers would
 * place again after its cancels, so close-all would not close all. TWAPs stop
 * first so they start no new chase slice; then every chase (slices included)
 * stops and cancels its resting order. Matched by exchange identity, never name.
 */
export const stopAccountAlgos = async (exchange: Exchange): Promise<NukeResult> => {
  const twaps = [...twapManager.runningTwaps.values()].filter((twap) => twap.exchange === exchange);
  for (const twap of twaps) await twap.terminate();
  const stoppedChasers = await chaserOrderManagerInstance.stopForNuke(exchange);
  return { stoppedChasers, stoppedTwaps: twaps.map((twap) => twap.id) };
};
