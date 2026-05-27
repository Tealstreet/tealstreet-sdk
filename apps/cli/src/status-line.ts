/**
 * Reserved top status line ("toaster"). Pins terminal row 1 (the bar) via an
 * ANSI scroll region (DECSTBM), keeps row 2 as a blank gap, and lets the REPL
 * output + readline prompt live in rows 3..N below it. Mirrors the latest
 * *notable* log event (warn/error/critical) on the bar with category styling,
 * auto-dimming after a few seconds.
 *
 * It is an ADDITIONAL glanceable surface — the full lines still print in
 * scrollback (Phase 2 writeAbovePrompt) and the file. Routine info/operational
 * lines are NOT shown here (they'd flicker the bar); only warn/error/critical.
 *
 * Design constraints learned in review:
 *  - Establish the region EARLY (before the startup banner) so banner/connect
 *    output flows into the region instead of fighting an after-the-fact cursor
 *    move. DECSTBM moves the cursor to the region's home on set, which is
 *    harmless when done before any prompt exists.
 *  - Cleanup is bulletproof via `process.on('exit')` — a leaked scroll region
 *    would leave the user's *shell* unable to use row 1.
 *  - Hard TTY guard: every public fn no-ops when stdout isn't a TTY (CI, pipes).
 *  - Watch modes (alt-screen) call suspend/resume so the region is reset before
 *    the alt buffer takes over and re-established on return.
 */
import chalk from 'chalk';

import type { LogCategory } from './log-classify';
import { addResizeListener, reserveTop, resumeRegion, suspendRegion } from './scroll-region';

const ESC = '\x1b';
const SAVE_CURSOR = `${ESC}7`;
const RESTORE_CURSOR = `${ESC}8`;
const CLEAR_ROW = `${ESC}[2K`;
const moveTo = (row: number, col: number): string => `${ESC}[${row};${col}H`;

/** Categories worth pinning to the status line. */
const NOTABLE: ReadonlySet<LogCategory> = new Set<LogCategory>(['warn', 'error', 'critical']);

const DIM_AFTER_MS = 5000;

// Row 1 = the alert bar, row 2 = a divider rule (drawn only once an alert
// exists; blank before that) separating the alert from the scrolling output.
// The scroll region therefore starts at row 3.
const REGION_TOP = 3;

let enabled = false;
let suspended = false;
let current: { text: string; category: LogCategory } | null = null;
let dimTimer: ReturnType<typeof setTimeout> | null = null;
let removeResizeListener: (() => void) | null = null;

const isTty = (): boolean => Boolean(process.stdout.isTTY);
const cols = (): number => process.stdout.columns || 80;
const rows = (): number => process.stdout.rows || 24;

/**
 * Build the styled row-1 string for a notable event. Pure (given a width) so it
 * can be unit-tested. Collapses whitespace, truncates to the terminal width
 * with an ellipsis, pads to the FULL width, and reverse-videos it (tinted by
 * category) so it reads as a pinned bar — not a normal output line. Dims once
 * stale.
 */
/**
 * Per-category bar palette. Explicit bg/fg hex (not the SGR `dim` attribute,
 * which only dims the foreground) so the whole bar — background included —
 * mutes when stale. `active` is bright; `stale` is the same hue, darkened.
 */
const BAR_STYLES: Record<
  'warn' | 'error' | 'critical',
  { activeBg: string; staleBg: string; activeFg: string; staleFg: string; bold?: boolean }
> = {
  warn: { activeBg: '#c9a227', staleBg: '#4a3d10', activeFg: '#1a1a1a', staleFg: '#b8a96a' },
  error: { activeBg: '#c0392b', staleBg: '#3d1a16', activeFg: '#ffffff', staleFg: '#c89a92' },
  critical: { activeBg: '#e01b1b', staleBg: '#4a0d0d', activeFg: '#ffffff', staleFg: '#c98a8a', bold: true },
};

export function formatStatusContent(text: string, category: LogCategory, width: number, dim: boolean): string {
  const icon = category === 'warn' ? '⚠' : category === 'critical' ? '⛔' : '✗';
  const prefix = `${icon} `;
  const clean = text.replace(/\s+/g, ' ').trim();
  const max = Math.max(0, width - prefix.length);
  const body = clean.length > max ? `${clean.slice(0, Math.max(0, max - 1))}…` : clean;
  // Clamp to EXACTLY the terminal width, then pad to it: the bar must never be
  // longer than one row, or it would wrap into the gap/scroll-region rows and
  // corrupt the layout (e.g. when width < the icon prefix). slice() handles the
  // tiny-width edge; padEnd() fills the rest so the background spans the row.
  const safeWidth = Math.max(0, width);
  const line = (prefix + body).slice(0, safeWidth).padEnd(safeWidth);
  const style = BAR_STYLES[category === 'warn' || category === 'critical' ? category : 'error'];
  const bg = dim ? style.staleBg : style.activeBg;
  const fg = dim ? style.staleFg : style.activeFg;
  let paint = chalk.bgHex(bg).hex(fg);
  if (style.bold) paint = paint.bold;
  return paint(line);
}

/** Reserve the top bar + divider rows via the scroll-region coordinator and blank them. */
function establishRegion(): void {
  reserveTop(REGION_TOP - 1); // rows 1 (bar) + 2 (divider)
  blankBarRows();
}

/**
 * Blank the pinned bar (row 1) + divider (row 2) without moving the prompt.
 * Only touches row 2 when the terminal actually has it, so we never emit a
 * cursor move to a non-existent row during a resize to height 1.
 */
function blankBarRows(): void {
  const row2 = rows() >= 2 ? moveTo(2, 1) + CLEAR_ROW : '';
  process.stdout.write(SAVE_CURSOR + moveTo(1, 1) + CLEAR_ROW + row2 + RESTORE_CURSOR);
}

function drawStatus(dim: boolean): void {
  // Skip when the terminal is too short to hold the bar (row 1) + divider
  // (row 2) — otherwise we'd emit a cursor move to a non-existent row 2 during
  // an aggressive resize. (The coordinator leaves the region full-screen here.)
  if (!enabled || suspended || !current || rows() < REGION_TOP) return;
  const styled = formatStatusContent(current.text, current.category, cols(), dim);
  // Row 1 = the alert bar; row 2 = a dimmed divider rule separating the alert
  // from the scrolling content below. The divider only appears once an alert
  // exists (before that, rows 1+2 are blank). Single write, cursor preserved.
  const divider = chalk.dim('─'.repeat(cols()));
  process.stdout.write(
    SAVE_CURSOR + moveTo(1, 1) + CLEAR_ROW + styled + moveTo(2, 1) + CLEAR_ROW + divider + RESTORE_CURSOR,
  );
}

/** Release the top reservation (coordinator preserves any footer reservation) and clear the rows. */
function teardownRegion(): void {
  reserveTop(0);
  blankBarRows();
}

/**
 * Pin row 1 and start mirroring notable events. Call EARLY in startRepl (before
 * the banner) so startup output flows into the region. Idempotent; no-op on
 * non-TTY.
 */
export function enableStatusLine(): void {
  if (enabled || !isTty()) return;
  enabled = true;
  establishRegion();

  // The scroll-region coordinator owns the resize + exit region handling
  // (including the too-short reset). We only redraw the bar's content at the
  // new width after a resize; it fires after the region is re-applied.
  removeResizeListener = addResizeListener(() => {
    if (!enabled || suspended) return;
    drawStatus(false);
  });
}

/** Restore the full-screen region and stop. Idempotent. */
export function disableStatusLine(): void {
  if (!enabled) return;
  enabled = false;
  if (dimTimer) {
    clearTimeout(dimTimer);
    dimTimer = null;
  }
  if (removeResizeListener) {
    removeResizeListener();
    removeResizeListener = null;
  }
  teardownRegion();
}

/** Reset the region before an alt-screen takeover (watch modes). */
export function suspendStatusLine(): void {
  if (!enabled || suspended) return;
  suspended = true;
  if (dimTimer) {
    clearTimeout(dimTimer);
    dimTimer = null;
  }
  suspendRegion();
}

/** Re-pin row 1 and redraw the last status after returning from alt-screen. */
export function resumeStatusLine(): void {
  if (!enabled || !suspended) return;
  suspended = false;
  resumeRegion();
  // The alt screen wiped the bar/gap rows — re-blank and redraw.
  blankBarRows();
  if (current) drawStatus(false);
}

/**
 * Mirror a notable event to the status line. Only warn/error/critical are
 * shown (info/operational would flicker the bar). No-op when disabled,
 * suspended, or non-TTY. Resets the auto-dim timer.
 */
export function setStatus(text: string, category: LogCategory): void {
  if (!enabled || !isTty() || !NOTABLE.has(category)) return;
  current = { text, category };
  drawStatus(false);
  if (dimTimer) clearTimeout(dimTimer);
  dimTimer = setTimeout(() => {
    drawStatus(true);
    dimTimer = null;
  }, DIM_AFTER_MS);
}
