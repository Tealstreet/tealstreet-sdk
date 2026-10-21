// Isolated synthetic identity entropy; never patches production crypto bindings.
import { createHash } from 'node:crypto';

export const createReplayEntropy = (tapeSha256) => {
  if (!/^[a-f0-9]{64}$/.test(tapeSha256 ?? '')) throw new Error('Replay entropy requires a tape SHA-256');
  let counter = 0n, block = Buffer.alloc(0), offset = 0;
  return function (length) {
    if (arguments.length !== 1 || !Number.isSafeInteger(length) || length < 0 || length > 65536) {
      throw new TypeError('Expected one byte length in [0, 65536]');
    }
    const bytes = new Uint8Array(length);
    for (let index = 0; index < bytes.byteLength; index++) {
      if (offset === block.length) {
        const sequence = Buffer.alloc(8);
        sequence.writeBigUInt64BE(counter++);
        block = createHash('sha256').update('tealstreet replay entropy v1\0')
          .update(tapeSha256, 'hex').update(sequence).digest();
        offset = 0;
      }
      bytes[index] = block[offset++];
    }
    return bytes;
  };
};
