import { afterEach, describe, expect, it } from 'vitest';

import { chaserOrderManagerInstance } from '@tealstreet/safe-cex/src/webworker/chaserOrder/ChaserOrderManager';

import { chaseAbandonedByExit } from './chaseAbandoned';

describe('chaseAbandonedByExit', () => {
  const instances = chaserOrderManagerInstance.instances as unknown as { isStopped: boolean }[];
  afterEach(() => instances.splice(0));

  it('names a chase the CLI manager is running, which the exchange store never lists', () => {
    instances.push({ isStopped: false });
    expect(chaseAbandonedByExit()).toMatch(/nothing is chasing now/);
  });

  it('says nothing once no chase is running', () => {
    expect(chaseAbandonedByExit()).toBeUndefined();
    instances.push({ isStopped: true });
    expect(chaseAbandonedByExit()).toBeUndefined();
  });
});
