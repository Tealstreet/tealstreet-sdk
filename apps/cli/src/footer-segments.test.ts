import { describe, expect, it } from 'vitest';

import { DEFAULT_SEGMENTS, renderFooter, renderSegment, type FooterSegment, type FooterState } from './footer-segments';

const strip = (s: string): string => s.replace(/\x1b\[[0-9;]*m/g, '');

const baseState = (over: Partial<FooterState> = {}): FooterState => ({
  connection: 'connected',
  account: 'bybit-main',
  symbol: 'BTCUSDT',
  version: '0.9.0',
  now: new Date('2026-01-02T14:22:09'),
  ...over,
});

const r = (seg: FooterSegment, state: FooterState): string => strip(renderSegment(seg, state));

describe('renderSegment', () => {
  it('connection shows a dot, plus a word for non-connected states', () => {
    expect(r({ type: 'connection' }, baseState({ connection: 'connected' }))).toBe('●');
    expect(r({ type: 'connection' }, baseState({ connection: 'connecting' }))).toBe('● connecting');
    expect(r({ type: 'connection' }, baseState({ connection: 'auth-failed' }))).toBe('● auth');
    expect(r({ type: 'connection' }, baseState({ connection: 'failed' }))).toBe('● offline');
  });

  it('account / symbol pass through, omit when absent', () => {
    expect(r({ type: 'account' }, baseState())).toBe('bybit-main');
    expect(r({ type: 'symbol' }, baseState())).toBe('BTCUSDT');
    expect(r({ type: 'account' }, baseState({ account: undefined }))).toBe('');
  });

  it('balance formats the total with a $ and thousands separators', () => {
    expect(r({ type: 'balance' }, baseState({ balanceTotal: 12480 }))).toBe('$12,480');
    expect(r({ type: 'balance' }, baseState({ balanceTotal: 84.5 }))).toBe('$84.5');
    expect(r({ type: 'balance' }, baseState({ balanceTotal: undefined }))).toBe('');
  });

  it('pnl renders signed $ + % (uPnL / |notional|) and omits with no position', () => {
    const withPos = baseState({
      position: { side: 'long', size: '0.5 BTC', entryPrice: 60000, notional: 32000, unrealizedPnl: 340 },
    });
    // 340 / 32000 * 100 = 1.06%
    expect(r({ type: 'pnl' }, withPos)).toBe('uPnL +$340 (+1.06%)');
    expect(r({ type: 'pnl' }, baseState({ position: null }))).toBe('');
    const loss = baseState({
      position: { side: 'short', size: '1 BTC', entryPrice: 100, notional: 100, unrealizedPnl: -12.5 },
    });
    expect(r({ type: 'pnl' }, loss)).toBe('uPnL -$12.5 (-12.50%)');
  });

  it('position shows side + base size and asset @ entry', () => {
    const s = baseState({
      position: { side: 'long', size: '240 DOGE', entryPrice: 0.1, notional: 24, unrealizedPnl: 1 },
    });
    expect(r({ type: 'position' }, s)).toBe('LONG 240 DOGE @ 0.1');
  });

  it('totalpnl, mark, last, funding, clock, text', () => {
    expect(r({ type: 'totalpnl' }, baseState({ totalUpnl: 1200 }))).toBe('Σ +$1,200');
    expect(r({ type: 'mark' }, baseState({ mark: 64210 }))).toBe('mark 64,210');
    expect(r({ type: 'last' }, baseState({ last: 64205 }))).toBe('64,205');
    expect(r({ type: 'funding' }, baseState({ fundingRate: 0.0001 }))).toBe('fund 0.0100%');
    expect(r({ type: 'clock' }, baseState())).toBe('14:22:09');
    expect(r({ type: 'text', text: 'hi' }, baseState())).toBe('hi');
    expect(r({ type: 'text' }, baseState())).toBe('');
  });

  it('version renders v<version>, omitted when unknown', () => {
    expect(r({ type: 'version' }, baseState())).toBe('v0.9.0');
    expect(r({ type: 'version' }, baseState({ version: undefined }))).toBe('');
  });

  it('script reads the cached value by name; empty until populated', () => {
    const withVal = baseState({ scriptValues: { myfooter: 'risk 1.2k' } });
    expect(r({ type: 'script', name: 'myfooter' }, withVal)).toBe('risk 1.2k');
    // No cached value yet, or no name → omitted.
    expect(r({ type: 'script', name: 'myfooter' }, baseState())).toBe('');
    expect(r({ type: 'script' }, withVal)).toBe('');
  });
});

describe('renderFooter', () => {
  it('groups left/right, drops empties, joins with the separator', () => {
    const segs: FooterSegment[] = [
      { type: 'connection' },
      { type: 'account' },
      { type: 'position' }, // empty (no position) → dropped
      { type: 'clock', group: 'right' },
    ];
    const { left, right } = renderFooter(segs, baseState(), '  ');
    // leading space + segments joined by the 2-space separator ('position' dropped).
    expect(strip(left)).toBe(' ●  bybit-main');
    expect(strip(right)).toBe('14:22:09 ');
  });

  it('empty groups yield empty strings (no stray padding)', () => {
    const { left, right } = renderFooter([], baseState());
    expect(left).toBe('');
    expect(right).toBe('');
  });

  it('DEFAULT_SEGMENTS renders a sensible default line', () => {
    const { left, right } = renderFooter(DEFAULT_SEGMENTS, baseState({ balanceTotal: 5000 }));
    expect(strip(left)).toContain('●');
    expect(strip(left)).toContain('bybit-main');
    expect(strip(left)).toContain('BTCUSDT');
    expect(strip(left)).toContain('$5,000');
    expect(strip(right)).toContain('v0.9.0');
    expect(strip(right)).toContain('14:22:09');
  });
});
