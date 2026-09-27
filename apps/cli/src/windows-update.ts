import { existsSync, renameSync, rmSync } from 'node:fs';

/**
 * Replacing a running executable on Windows.
 *
 * Windows holds a lock on the image of a running process: you cannot delete or
 * overwrite `tealstreet.exe` while it is executing, which is why the POSIX
 * `mv new current` path cannot be reused. You CAN rename it — the open handle
 * follows the rename — so the swap is:
 *
 *   1. download beside the current binary (same volume, so renames are atomic)
 *   2. rename the running binary aside to `.old`
 *   3. rename the download into its place
 *   4. delete the `.old` on the next launch, once nothing holds it
 *
 * Step 2 is the only irreversible-looking moment, and it is why step 3 rolls
 * back on failure: a half-applied update that left no `tealstreet.exe` would
 * take the user's install with it.
 */

export interface UpdatePaths {
  /** Where the freshly downloaded binary is staged. */
  next: string;
  /** Where the running binary is parked so its name can be reused. */
  previous: string;
}

export function updatePaths(currentBinary: string): UpdatePaths {
  return {
    next: `${currentBinary}.new`,
    previous: `${currentBinary}.old`,
  };
}

/**
 * Swap a staged download into place. Throws with the original binary restored
 * if the second rename fails.
 */
export function swapInPlace(currentBinary: string, staged: string): void {
  if (!existsSync(staged)) {
    throw new Error(`Staged update is missing: ${staged}`);
  }

  const { previous } = updatePaths(currentBinary);

  // A leftover `.old` from an update that was never cleaned up would make the
  // rename fail on Windows, where rename does not overwrite.
  rmSync(previous, { force: true });

  renameSync(currentBinary, previous);
  try {
    renameSync(staged, currentBinary);
  } catch (error) {
    // Put the user's working binary back before surfacing the failure.
    renameSync(previous, currentBinary);
    throw error;
  }
}

/**
 * Remove the parked binary from a previous update. Best-effort by design: on
 * the very first launch after an update Windows may still hold the old image
 * briefly, and failing to delete it must never stop the CLI from starting.
 */
export function cleanupStaleUpdate(currentBinary: string = process.execPath): void {
  if (process.platform !== 'win32') return;
  try {
    const { next, previous } = updatePaths(currentBinary);
    rmSync(previous, { force: true });
    // A `.new` still sitting here means a download died before the swap.
    if (existsSync(next) && existsSync(currentBinary)) {
      rmSync(next, { force: true });
    }
  } catch {
    // Ignore — a stale file is harmless, a failed startup is not.
  }
}
