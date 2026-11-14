import type { Exchange } from '../../exchanges/base';

import { readFileSync } from 'node:fs';

import { RustTwapOwnership } from '../../webworker/twap/RustTwapOwnership';
import { setTwapLockManager } from '../../webworker/twap/twapLock';

// The native contract test consumes the actual wrapper output, not a saved projection.
const main = async () => {
  const { rows, now } = JSON.parse(readFileSync(0, 'utf8'));
  Date.now = () => now;
  setTwapLockManager(null);
  const forwarded: unknown[] = [];
  const ownership = new RustTwapOwnership(
    { emitter: { emit() {} } } as unknown as Exchange,
    (op, data) => {
      if (op !== 'twap:state') throw new Error(`Unexpected op: ${op}`);
      forwarded.push(...(data as unknown[]));
    },
    () => {},
    () => ({ ownerId: 'A', generation: 7 }),
  );
  try {
    ownership.handle('twap:state', rows);
    for (let i = 0; i < 8; i++) await Promise.resolve();
    process.stdout.write(JSON.stringify(forwarded));
  } finally {
    ownership.dispose();
  }
};

void main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
