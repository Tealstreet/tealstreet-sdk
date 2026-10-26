// Synthetic TS client-ID entropy only; signing retains the real crypto source.
import { createHash } from 'node:crypto';

export const createLighterReplayRandom = (tapeSha256) => {
  if (!/^[a-f0-9]{64}$/.test(tapeSha256 ?? '')) throw new Error('Lighter replay random requires a tape SHA-256');
  let counter = 0n;
  return () => {
    const sequence = Buffer.alloc(8);
    sequence.writeBigUInt64BE(counter++);
    const digest = createHash('sha256').update('tealstreet lighter ts replay random v1\0')
      .update(tapeSha256, 'hex').update(sequence).digest();
    return Number(digest.readBigUInt64BE(0) >> 11n) / 2 ** 53;
  };
};
