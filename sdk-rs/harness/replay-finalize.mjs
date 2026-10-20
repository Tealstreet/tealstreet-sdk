import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { deserialize, serialize } from 'node:v8';

// Unwind this synchronous scope before GC: comparison objects and the binary
// buffer belong to the output artifact, not the live implementation's heap.
const spool = (capture, path) => { writeFileSync(path, serialize(capture())); };

export async function finalizeReplay(capture, measure, { check = () => {}, directory = tmpdir() } = {}) {
  const folder = mkdtempSync(join(directory, 'safe-cex-replay-'));
  const path = join(folder, 'comparison.bin');
  try {
    spool(capture, path);
    const metrics = await measure();
    check(); // Late authentication/protocol failures must remain fatal.
    return { ...deserialize(readFileSync(path)), metrics };
  } finally { rmSync(folder, { recursive: true, force: true }); }
}
