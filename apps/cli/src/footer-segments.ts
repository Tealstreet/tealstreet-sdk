/**
 * Built-in footer segments. Pure rendering: a normalized `FooterState`
 * snapshot in, a styled string out — so it's fully unit-testable and decoupled
 * from safe-cex types. The impure snapshot builder (reading exchangeManager +
 * connectionStates) lives in repl.ts and feeds these via `setFooterRenderer`.
 *
 * Each segment renders to a short token or '' (omitted when there's no data —
 * e.g. no open position, balance not streamed yet). Segments are grouped left
 * / right and joined with a separator by `renderFooter`.
 *
 * PnL % matches the rest of the CLI: unrealizedPnl / |notional| × 100
 * (PositionCommand.ts / MonitorCommand.ts).
 */
import chalk from 'chalk';

export type FooterConnection = 'connected' | 'connecting' | 'auth-failed' | 'failed' | 'disconnected' | 'unknown';

export interface FooterPosition {
  side: 'long' | 'short';
  /** Base size with its asset, formatted by the snapshot builder (e.g. `240 DOGE`). */
  size: string;
  entryPrice: number;
  notional: number;
  unrealizedPnl: number;
}

/** Normalized snapshot the segment renderers read. All fields optional. */
export interface FooterState {
  connection: FooterConnection;
  account?: string;
  symbol?: string;
  balanceFree?: number;
  balanceTotal?: number;
  position?: FooterPosition | null;
  totalUpnl?: number;
  mark?: number;
  last?: number;
  fundingRate?: number;
  /** CLI version string (no leading 'v'), for the `version` segment. */
  version?: string;
  /** Latest cached output of script-backed segments, keyed by script name. */
  scriptValues?: Record<string, string>;
  now: Date;
}

export type FooterSegmentType =
  | 'connection'
  | 'account'
  | 'symbol'
  | 'balance'
  | 'pnl'
  | 'totalpnl'
  | 'position'
  | 'mark'
  | 'last'
  | 'funding'
  | 'clock'
  | 'version'
  | 'text'
  | 'script';

export interface FooterSegment {
  type: FooterSegmentType;
  /** Which side of the footer. Default 'left'. */
  group?: 'left' | 'right';
  /** Literal content for type:'text'. */
  text?: string;
  /** Script name for type:'script' (the script's return value is rendered). */
  name?: string;
  /** Optional chalk hex/named colour override for the value. */
  color?: string;
}

/** Runtime list of valid segment types (for the `footer add` command). */
export const FOOTER_SEGMENT_TYPES: FooterSegmentType[] = [
  'connection',
  'account',
  'symbol',
  'balance',
  'pnl',
  'totalpnl',
  'position',
  'mark',
  'last',
  'funding',
  'clock',
  'version',
  'text',
  'script',
];

/** The default layout when the user hasn't configured segments. */
export const DEFAULT_SEGMENTS: FooterSegment[] = [
  { type: 'connection' },
  { type: 'account' },
  { type: 'symbol' },
  { type: 'balance' },
  { type: 'pnl' },
  { type: 'version', group: 'right' },
  { type: 'clock', group: 'right' },
];

const CONNECTION_STYLE: Record<FooterConnection, { color: string; word: string }> = {
  connected: { color: '#3fb950', word: '' },
  connecting: { color: '#d29922', word: ' connecting' },
  'auth-failed': { color: '#f85149', word: ' auth' },
  failed: { color: '#f85149', word: ' offline' },
  disconnected: { color: '#8b949e', word: ' offline' },
  unknown: { color: '#8b949e', word: '' },
};

const POS_GREEN = '#3fb950';
const POS_RED = '#f85149';

function fmtUsd(n: number): string {
  const abs = Math.abs(n);
  const num = abs.toLocaleString('en-US', { maximumFractionDigits: abs >= 1000 ? 0 : 2 });
  return `${n < 0 ? '-' : ''}$${num}`;
}

function fmtSignedUsd(n: number): string {
  const abs = Math.abs(n);
  const num = abs.toLocaleString('en-US', { maximumFractionDigits: abs >= 1000 ? 0 : 2 });
  return `${n < 0 ? '-' : '+'}$${num}`;
}

function fmtPrice(n: number): string {
  return n.toLocaleString('en-US', { maximumFractionDigits: n >= 100 ? 2 : 6 });
}

function fmtSignedPct(n: number): string {
  return `${n >= 0 ? '+' : ''}${n.toFixed(2)}%`;
}

const pnlColor = (n: number): string => (n > 0 ? POS_GREEN : n < 0 ? POS_RED : '#8b949e');

const label = (s: string): string => chalk.dim(s);

/**
 * Apply an optional per-segment colour. `seg.color` comes from user config, and
 * `chalk.hex` THROWS on a non-hex value (e.g. `"red"` or a typo) — which, on the
 * footer's redraw timer, would crash the REPL. So validate a hex string and
 * fall back to no colour on anything else.
 */
const HEX_RE = /^#?[0-9a-fA-F]{3,8}$/;
const colorize = (color: string | undefined, s: string): string =>
  color && HEX_RE.test(color) ? chalk.hex(color)(s) : s;

/** Render one segment to a styled token, or '' to omit it. */
export function renderSegment(seg: FooterSegment, state: FooterState): string {
  const paint = (s: string): string => colorize(seg.color, s);
  switch (seg.type) {
    case 'text':
      return seg.text ? paint(seg.text) : '';

    case 'connection': {
      const { color, word } = CONNECTION_STYLE[state.connection];
      return chalk.hex(color)(`●${word}`);
    }

    case 'account':
      return state.account ? paint(state.account) : '';

    case 'symbol':
      return state.symbol ? paint(state.symbol) : '';

    case 'balance':
      return state.balanceTotal != null ? paint(fmtUsd(state.balanceTotal)) : '';

    case 'pnl': {
      const pos = state.position;
      if (!pos) return '';
      const pct =
        pos.entryPrice > 0 && pos.notional !== 0 ? (pos.unrealizedPnl / Math.abs(pos.notional)) * 100 : undefined;
      const body = `uPnL ${fmtSignedUsd(pos.unrealizedPnl)}${pct != null ? ` (${fmtSignedPct(pct)})` : ''}`;
      return chalk.hex(pnlColor(pos.unrealizedPnl))(body);
    }

    case 'totalpnl':
      if (state.totalUpnl == null) return '';
      return chalk.hex(pnlColor(state.totalUpnl))(`Σ ${fmtSignedUsd(state.totalUpnl)}`);

    case 'position': {
      const pos = state.position;
      if (!pos) return '';
      const color = pos.side === 'long' ? POS_GREEN : POS_RED;
      const tag = pos.side === 'long' ? 'LONG' : 'SHORT';
      return chalk.hex(color)(`${tag} ${pos.size} @ ${fmtPrice(pos.entryPrice)}`);
    }

    case 'mark':
      return state.mark != null ? `${label('mark')} ${paint(fmtPrice(state.mark))}` : '';

    case 'last':
      return state.last != null ? paint(fmtPrice(state.last)) : '';

    case 'funding':
      return state.fundingRate != null ? `${label('fund')} ${paint(`${(state.fundingRate * 100).toFixed(4)}%`)}` : '';

    case 'clock': {
      const d = state.now;
      const hh = String(d.getHours()).padStart(2, '0');
      const mm = String(d.getMinutes()).padStart(2, '0');
      const ss = String(d.getSeconds()).padStart(2, '0');
      return paint(`${hh}:${mm}:${ss}`);
    }

    case 'version':
      // Dimmed by default (it was gray in the old prompt prefix); a `color`
      // override still wins via `paint`.
      return state.version ? (seg.color ? paint(`v${state.version}`) : label(`v${state.version}`)) : '';

    case 'script': {
      // The cached return value of the named script (refreshed off-tick by the
      // host; see repl.ts runFooterScript). Empty until the first run resolves.
      const v = seg.name ? state.scriptValues?.[seg.name] : undefined;
      return v ? paint(v) : '';
    }

    default:
      return '';
  }
}

/**
 * Render the full footer into left + right groups. Each segment is rendered,
 * empties dropped, and the survivors joined with `separator`. The left group
 * gets a leading space (off the col-1 edge); the right group a trailing space.
 */
export function renderFooter(
  segments: FooterSegment[],
  state: FooterState,
  separator = '  ',
): { left: string; right: string } {
  const left: string[] = [];
  const right: string[] = [];
  for (const seg of segments) {
    const out = renderSegment(seg, state);
    if (!out) continue;
    (seg.group === 'right' ? right : left).push(out);
  }
  const leftStr = left.length ? ` ${left.join(separator)}` : '';
  const rightStr = right.length ? `${right.join(separator)} ` : '';
  return { left: leftStr, right: rightStr };
}
