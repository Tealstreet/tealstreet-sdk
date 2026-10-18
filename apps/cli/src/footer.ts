/**
 * Persistent bottom "footer" — a Claude-Code-style status line pinned to the
 * last terminal row, showing always-on live data (connection, account, balance,
 * PnL, …). Counterpart to the top alert bar (`status-line.ts`): the top bar is
 * transient (warn/error events that auto-dim), the footer is always-on.
 *
 * Surface mechanics (shared with the top bar via `scroll-region.ts`):
 *  - Reserves the bottom TWO rows through the coordinator (a divider rule +
 *    the content line), so the scroll region becomes `[topReserved+1, rows-2]`.
 *    The readline prompt then sits at the bottom of the region — above the
 *    divider + footer — and scrolling output never touches them.
 *  - Redraws on a timer (live data changes without user input) and after a
 *    resize (row index + width change). Cursor save/restore so an in-progress
 *    prompt is never disturbed. The content draw is a deduped, full-width,
 *    single-write OVERWRITE (no clear-then-write) so it never blinks: identical
 *    frames are skipped, and a changed frame replaces the row in one write.
 *  - Suspends/resumes around watch-mode alt-screen, like the top bar.
 *  - Hard TTY guard: every public fn no-ops when stdout isn't a TTY.
 *
 * Known limitation: a multi-line (wrapped) prompt can transiently draw over the
 * footer row, since readline is unaware of the reserved region; the next timer
 * tick repaints it. Acceptable for v1 — prompts are short in practice.
 *
 * Phase 2 ships the surface with a placeholder renderer; Phase 3 injects the
 * real segment renderer via `setFooterRenderer`.
 */
import chalk from 'chalk';

import { addResizeListener, reserveBottom, resumeRegion, suspendRegion } from './scroll-region';
import { BRAND } from './brand';

const ESC = '\x1b';
const SAVE_CURSOR = `${ESC}7`;
const RESTORE_CURSOR = `${ESC}8`;
const CLEAR_ROW = `${ESC}[2K`;
const moveTo = (row: number, col: number): string => `${ESC}[${row};${col}H`;

const ANSI_RE = /\x1b\[[0-9;]*m/g;

// Reserve two bottom rows: a divider rule + the content line. The divider
// separates the scrolling output / prompt from the pinned footer.
const FOOTER_HEIGHT = 2;
const DEFAULT_REDRAW_MS = 1000;

let enabled = false;
let suspended = false;
let redrawTimer: ReturnType<typeof setInterval> | null = null;
let removeResizeListener: (() => void) | null = null;
let redrawMs = DEFAULT_REDRAW_MS;
/** Last content line written, for flicker-free dedup (skip identical redraws). */
let lastContentLine: string | null = null;

/** Row indices (1-based): divider sits just above the content line. */
const dividerRow = (): number => rows() - 1;
const contentRow = (): number => rows();

const isTty = (): boolean => Boolean(process.stdout.isTTY);
const rows = (): number => process.stdout.rows || 24;
const cols = (): number => process.stdout.columns || 80;

/** Visible (printable) length of a string, ignoring ANSI SGR colour runs. */
export function visibleLength(s: string): number {
  return s.replace(ANSI_RE, '').length;
}

/**
 * Compose one footer row: `left` group at the left edge, `right` group flush
 * right, single-space-minimum gap between. Pure + ANSI-aware (measures visible
 * width so colour codes don't throw off alignment) so it's unit-testable. On
 * overflow it falls back to a plain (colour-stripped) clamp with an ellipsis —
 * the row must never exceed `width` or it would wrap onto the prompt/scroll
 * rows. The caller pads the result to full width and overwrites in place (no
 * clear), so this returns the un-padded composed line.
 */
export function formatFooterLine(left: string, right: string, width: number): string {
  if (width <= 0) return '';
  const lLen = visibleLength(left);
  const rLen = visibleLength(right);
  const needGap = rLen > 0 ? 1 : 0;
  if (lLen + rLen + needGap <= width) {
    if (!right) return left;
    return left + ' '.repeat(width - lLen - rLen) + right;
  }
  // Overflow — clamp the plain text so we never miscount a coloured run.
  const plain = (right ? `${left} ${right}` : left).replace(ANSI_RE, '');
  if (plain.length <= width) return plain;
  return plain.slice(0, Math.max(0, width - 1)) + '…';
}

/** What the footer should show. Phase 3 replaces this with the segment engine. */
let renderer: () => { left: string; right: string } = () => ({
  left: ` ${BRAND.binName} · footer ready`,
  right: '',
});

/** Inject the content renderer (Phase 3 segment engine). Redraws immediately. */
export function setFooterRenderer(fn: () => { left: string; right: string }): void {
  renderer = fn;
  drawFooter();
}

/**
 * Enough rows to place the divider (row N-1) + content (row N) without
 * addressing row 0. The coordinator already leaves the region full-screen when
 * the terminal is this short; this guards the direct draws too.
 */
const hasRoom = (): boolean => rows() >= FOOTER_HEIGHT;

function drawFooter(): void {
  if (!enabled || suspended || !isTty() || !hasRoom()) return;
  // The renderer is host-injected and runs on a timer + after readline refresh;
  // if it throws, swallow it so the exception can't escape the interval tick or
  // readline's _refreshLine and crash the CLI.
  let left = '';
  let right = '';
  try {
    ({ left, right } = renderer());
  } catch {
    return;
  }
  const width = cols();
  // Pad to the FULL width so a single write overwrites the whole row in place —
  // no CLEAR_ROW first, which is what caused the per-tick blink (clear → brief
  // empty frame → rewrite). Dedup identical frames so a static footer never
  // repaints at all.
  const content = formatFooterLine(left, right, width);
  const line = content + ' '.repeat(Math.max(0, width - visibleLength(content)));
  if (line === lastContentLine) return;
  lastContentLine = line;
  process.stdout.write(SAVE_CURSOR + moveTo(contentRow(), 1) + line + RESTORE_CURSOR);
}

/** Draw the divider rule above the footer (full-width, dimmed). Static (resize only). */
function drawDivider(): void {
  if (!enabled || suspended || !isTty() || !hasRoom()) return;
  const rule = chalk.dim('─'.repeat(cols()));
  process.stdout.write(SAVE_CURSOR + moveTo(dividerRow(), 1) + rule + RESTORE_CURSOR);
}

/** Clear both reserved bottom rows without disturbing the prompt cursor. */
function blankFooterRows(): void {
  lastContentLine = null;
  if (!isTty() || !hasRoom()) return;
  process.stdout.write(
    SAVE_CURSOR + moveTo(dividerRow(), 1) + CLEAR_ROW + moveTo(contentRow(), 1) + CLEAR_ROW + RESTORE_CURSOR,
  );
}

/**
 * Pin the bottom row and start the redraw loop. Idempotent; no-op on non-TTY.
 * `intervalMs` overrides the redraw cadence (clamped to a sane floor).
 */
export function enableFooter(intervalMs?: number): void {
  if (enabled || !isTty()) return;
  enabled = true;
  redrawMs = Math.max(200, intervalMs ?? DEFAULT_REDRAW_MS);

  // Coordinator owns the resize + exit region handling; we redraw content (the
  // footer row index and width both move on resize) after it re-applies.
  reserveBottomRows();
  removeResizeListener = addResizeListener(() => {
    if (!enabled || suspended) return;
    lastContentLine = null; // width/row changed — force a repaint
    drawDivider();
    drawFooter();
  });

  drawDivider();
  drawFooter();
  redrawTimer = setInterval(drawFooter, redrawMs);
  // Don't keep the process alive solely for the footer tick.
  redrawTimer.unref?.();
}

function reserveBottomRows(): void {
  reserveBottom(FOOTER_HEIGHT);
  blankFooterRows();
}

/** Unpin the footer row and stop redrawing. Idempotent. */
export function disableFooter(): void {
  if (!enabled) return;
  enabled = false;
  if (redrawTimer) {
    clearInterval(redrawTimer);
    redrawTimer = null;
  }
  if (removeResizeListener) {
    removeResizeListener();
    removeResizeListener = null;
  }
  reserveBottom(0); // release the bottom reservation (top bar's is preserved)
  blankFooterRows();
}

/** Reset the region before an alt-screen takeover (watch modes). */
export function suspendFooter(): void {
  if (!enabled || suspended) return;
  suspended = true;
  suspendRegion();
}

/** Re-pin the footer row and redraw after returning from alt-screen. */
export function resumeFooter(): void {
  if (!enabled || !suspended) return;
  suspended = false;
  resumeRegion();
  lastContentLine = null; // alt screen wiped the rows — force a repaint
  blankFooterRows();
  drawDivider();
  drawFooter();
}

/**
 * Force-repaint the divider + content. Used after readline's `_refreshLine`,
 * whose `clearScreenDown` wipes everything below the prompt — i.e. our divider
 * and footer rows. Resets the dedup cache (the on-screen rows were just
 * cleared, so the cached "last line" no longer reflects what's visible) and
 * redraws both. No-op when disabled / suspended / non-TTY.
 */
export function redrawFooter(): void {
  if (!enabled || suspended || !isTty()) return;
  lastContentLine = null;
  drawDivider();
  drawFooter();
}

export function isFooterEnabled(): boolean {
  return enabled;
}
