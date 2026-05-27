/**
 * Single owner of the terminal's DECSTBM scroll region (`ESC[top;bottomr`).
 *
 * A terminal has exactly ONE scroll region, so the top alert bar
 * (`status-line.ts`) and the bottom footer (`footer.ts`) cannot each set it
 * independently — they'd clobber each other. Instead both register how many
 * rows they reserve at the top / bottom here, and this module computes the one
 * combined region and applies it.
 *
 * Reservation math (1-indexed rows): with `topReserved` rows pinned at the top
 * and `bottomReserved` at the bottom, the scrollable region is
 * `[topReserved + 1, rows - bottomReserved]`. The status bar reserves 2 (bar +
 * gap); a 1-row footer reserves 1. When nothing is reserved, or the terminal is
 * too short to leave any scrollable rows, the region is reset to full-screen so
 * a clipped region can never strand the prompt or the user's shell.
 *
 * Surfaces keep owning their own CONTENT (drawing into their reserved rows) and
 * register a resize callback here so they redraw after the region is re-applied.
 * This module owns the single `resize` + `process 'exit'` handlers; the exit
 * reset is the bulletproof bit — a leaked region breaks the user's shell.
 */
const ESC = '\x1b';
const RESET_REGION = `${ESC}[r`;
const SAVE_CURSOR = `${ESC}7`;
const RESTORE_CURSOR = `${ESC}8`;
const setRegionSeq = (top: number, bottom: number): string => `${ESC}[${top};${bottom}r`;

let topReserved = 0;
let bottomReserved = 0;
let applied = false;
let suspended = false;
let handlersAttached = false;
const resizeListeners = new Set<() => void>();

const isTty = (): boolean => Boolean(process.stdout.isTTY);
const rows = (): number => process.stdout.rows || 24;

/**
 * Pure region math (exported for unit tests). Returns the 1-indexed
 * `[top, bottom]` scroll region, or `null` when the region should be
 * full-screen: nothing reserved, or not enough room for at least one
 * scrollable row.
 */
export function computeRegion(totalRows: number, top: number, bottom: number): [number, number] | null {
  const t = Math.max(0, top);
  const b = Math.max(0, bottom);
  if (t === 0 && b === 0) return null;
  const regionTop = t + 1;
  const regionBottom = totalRows - b;
  if (regionBottom < regionTop) return null;
  return [regionTop, regionBottom];
}

function applyRegion(): void {
  if (!isTty() || suspended) return;
  const region = computeRegion(rows(), topReserved, bottomReserved);
  if (!region) {
    if (applied) {
      process.stdout.write(RESET_REGION);
      applied = false;
    }
    return;
  }
  // Wrap in save/restore so applying the region (DECSTBM homes the cursor as a
  // side effect) doesn't yank a live prompt's cursor around on resize.
  process.stdout.write(SAVE_CURSOR + setRegionSeq(region[0], region[1]) + RESTORE_CURSOR);
  applied = true;
}

function onResize(): void {
  applyRegion();
  for (const cb of resizeListeners) {
    try {
      cb();
    } catch {
      // A surface's redraw must never break the resize handler for the others.
    }
  }
}

function onExit(): void {
  if (applied) {
    process.stdout.write(RESET_REGION);
    applied = false;
  }
}

function ensureHandlers(): void {
  if (handlersAttached || !isTty()) return;
  handlersAttached = true;
  process.stdout.on('resize', onResize);
  process.on('exit', onExit);
}

/** Reserve `n` rows at the top of the screen and re-apply the region. */
export function reserveTop(n: number): void {
  topReserved = Math.max(0, n);
  ensureHandlers();
  applyRegion();
}

/** Reserve `n` rows at the bottom of the screen and re-apply the region. */
export function reserveBottom(n: number): void {
  bottomReserved = Math.max(0, n);
  ensureHandlers();
  applyRegion();
}

/**
 * Register a callback fired after the region is re-applied on resize, so a
 * surface can redraw its content at the (possibly new) width. Returns an
 * unsubscribe function.
 */
export function addResizeListener(cb: () => void): () => void {
  resizeListeners.add(cb);
  return () => resizeListeners.delete(cb);
}

/**
 * Reset the region to full-screen WITHOUT clearing reservations, e.g. before an
 * alt-screen takeover (watch modes). `resumeRegion()` re-applies it.
 */
export function suspendRegion(): void {
  if (suspended) return;
  suspended = true;
  if (applied) {
    process.stdout.write(RESET_REGION);
    applied = false;
  }
}

/** Re-apply the region after returning from an alt-screen takeover. */
export function resumeRegion(): void {
  if (!suspended) return;
  suspended = false;
  applyRegion();
}

/** Hard reset: drop all reservations and restore the full-screen region. */
export function resetRegion(): void {
  topReserved = 0;
  bottomReserved = 0;
  if (applied) {
    process.stdout.write(RESET_REGION);
    applied = false;
  }
}

/** Test-only: reset module state without touching the real terminal. */
export function __resetScrollRegionForTest(): void {
  // Detach the real resize/exit handlers + reset the attached flag too, or
  // tests leak listeners and become order-dependent (duplicate callbacks).
  if (handlersAttached) {
    process.stdout.off('resize', onResize);
    process.off('exit', onExit);
    handlersAttached = false;
  }
  topReserved = 0;
  bottomReserved = 0;
  applied = false;
  suspended = false;
  resizeListeners.clear();
}
