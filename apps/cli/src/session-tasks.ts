// A new CLI login must not inherit previous-session supervisors or task rows.
import { chaserOrderManagerInstance } from '@tealstreet/safe-cex/src/webworker/chaserOrder/ChaserOrderManager';
import { twapManager } from '@tealstreet/safe-cex/src/webworker/twap/TwapManager';
import { clearTriggers } from '@tealstreet/cli-core/src/triggers/triggerRegistry';
import { onStorageOwnerChange } from './storage-context';

onStorageOwnerChange(() => {
  clearTriggers();
  for (const chase of [...chaserOrderManagerInstance.instances]) {
    // Stop supervision without issuing a new cancel against the venue.
    chaserOrderManagerInstance.removeKeepOrder(chase.id);
  }
  const twaps = [...twapManager.runningTwaps.values()];
  twapManager.runningTwaps.clear();
  for (const twap of twaps) void twap.terminate().catch(() => {});
});
