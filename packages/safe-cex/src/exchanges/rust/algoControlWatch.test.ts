import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { OrderSide } from '../../types';
import { TwapStatus } from '../../webworker/twap/TwapStatus';
import { AlgoControlWatch } from './algoControlWatch';

const chaser = (id: string, isStopped = false) => ({ id, symbol: 'BTCUSDT', side: OrderSide.Buy, isStopped }) as never;
const twap = (id: string, status: TwapStatus) => ({ id, status }) as never;

const setup = () => {
  const rows = { activeChasers: [] as never[], activeTWAPs: [] as never[] };
  const listeners = new Set<() => void>();
  const sent: Array<[string, unknown]> = [];
  const told: string[] = [];
  const watch = new AlgoControlWatch({
    rows: () => rows,
    subscribe: (listener) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    resend: (op, data) => {
      sent.push([op, data]);
      watch.observe(op, data);
    },
    tell: (message) => told.push(message),
  });
  const update = (next: Partial<typeof rows>) => {
    Object.assign(rows, next);
    [...listeners].forEach((listener) => listener());
  };
  return { rows, sent, told, watch, update, listeners };
};

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

describe('a chase or TWAP stop', () => {
  it('is sent again until the rows confirm it, and says nothing when it lands', () => {
    const { rows, sent, told, watch, update, listeners } = setup();
    rows.activeChasers = [chaser('c1')];
    watch.observe('chaser:terminate', 'c1');
    vi.advanceTimersByTime(5_000);
    expect(sent).toEqual([['chaser:terminate', 'c1']]);
    update({ activeChasers: [] });
    vi.advanceTimersByTime(120_000);
    expect(sent).toHaveLength(1);
    expect(told).toEqual([]);
    expect(listeners.size).toBe(0);
  });

  it('a second click during the retries adds no second watch, and a later stop watches again', () => {
    const { rows, sent, told, watch, update } = setup();
    rows.activeChasers = [chaser('c1')];
    watch.observe('chaser:terminate', 'c1');
    watch.observe('chaser:terminate', 'c1');
    vi.advanceTimersByTime(15_000);
    expect(sent).toHaveLength(3);
    expect(told).toHaveLength(1);
    update({ activeChasers: [] });
    rows.activeChasers = [chaser('c1')];
    watch.observe('chaser:terminate', 'c1');
    vi.advanceTimersByTime(5_000);
    expect(sent).toHaveLength(4);
  });

  it('counts a stopped row as confirmed, since cancels may still be settling', () => {
    const { rows, sent, watch, update } = setup();
    rows.activeChasers = [chaser('c1')];
    watch.observe('chaser:terminate', 'c1');
    update({ activeChasers: [chaser('c1', true)] });
    vi.advanceTimersByTime(60_000);
    expect(sent).toEqual([]);
  });

  it('says it is unconfirmed at 15 s and NOT confirmed at 60 s, then stops sending', () => {
    const { rows, sent, told, watch } = setup();
    rows.activeTWAPs = [twap('t1', TwapStatus.Running)];
    watch.observe('twap:stop', 't1');
    vi.advanceTimersByTime(14_999);
    expect(told).toEqual([]);
    vi.advanceTimersByTime(1);
    expect(told[0]).toContain('not confirmed yet');
    vi.advanceTimersByTime(45_000);
    expect(told[1]).toContain('NOT confirmed');
    expect(sent).toHaveLength(11);
    expect(sent.every(([op, id]) => op === 'twap:stop' && id === 't1')).toBe(true);
    vi.advanceTimersByTime(60_000);
    expect(sent).toHaveLength(11);
    expect(told).toHaveLength(2);
  });

  it('confirms a TWAP stop on completed, and ignores a stop for a row already gone', () => {
    const { rows, sent, watch, update } = setup();
    watch.observe('twap:stop', 'gone');
    rows.activeTWAPs = [twap('t1', TwapStatus.Running)];
    watch.observe('twap:stop', 't1');
    update({ activeTWAPs: [twap('t1', TwapStatus.Completed)] });
    vi.advanceTimersByTime(60_000);
    expect(sent).toEqual([]);
  });
});

describe('a stop the user was told might not have landed', () => {
  it('ends with how the chase ended, even after NOT confirmed', () => {
    const { rows, sent, told, watch } = setup();
    rows.activeChasers = [chaser('c1')];
    watch.observe('chaser:terminate', 'c1');
    vi.advanceTimersByTime(60_000);
    expect(told.at(-1)).toContain('NOT confirmed');
    const resent = sent.length;
    // The connection returns; the server replays the chase's end.
    watch.chaseCompleted({ id: 'c1', symbol: 'BTCUSDT', size: 1, filled: 1 });
    expect(told.at(-1)).toBe('Chase on BTCUSDT filled 1 before its stop took effect.');
    vi.advanceTimersByTime(120_000);
    expect(sent).toHaveLength(resent);
    expect(told).toHaveLength(3);
  });

  it('adds nothing when the chaser already told how its retried stop ended', () => {
    const { rows, told, watch, update } = setup();
    rows.activeChasers = [chaser('c1')];
    watch.observe('chaser:terminate', 'c1');
    vi.advanceTimersByTime(60_000);
    const before = told.length;
    // The connection returns: the row goes, then the end the chaser already told.
    update({ activeChasers: [] });
    watch.chaseCompleted({ id: 'c1', symbol: 'BTCUSDT', size: 1, filled: 0, outcomeTold: true });
    vi.advanceTimersByTime(120_000);
    expect(told).toHaveLength(before);
  });

  it('says it stopped, and what filled, when the end comes during the retries', () => {
    const { rows, told, watch, update } = setup();
    rows.activeChasers = [chaser('c1')];
    watch.observe('chaser:terminate', 'c1');
    vi.advanceTimersByTime(15_000);
    watch.chaseCompleted({ id: 'c1', symbol: 'BTCUSDT', size: 1, filled: 0.4 });
    update({ activeChasers: [] });
    vi.advanceTimersByTime(60_000);
    expect(told).toEqual([
      'Chase stop is not confirmed yet; retrying. It may still be running.',
      'Chase on BTCUSDT stopped after filling 0.4 of 1.',
    ]);
  });

  it('says the fill is unknown when the rows confirm it and no end follows', () => {
    const { rows, told, watch, update } = setup();
    rows.activeChasers = [chaser('c1')];
    watch.observe('chaser:terminate', 'c1');
    vi.advanceTimersByTime(60_000);
    update({ activeChasers: [] });
    vi.advanceTimersByTime(9_999);
    expect(told).toHaveLength(2);
    vi.advanceTimersByTime(1);
    expect(told.at(-1)).toContain('whether it filled is not known');
  });

  it('a stop confirmed before any warning says nothing of its end', () => {
    const { rows, told, watch, update } = setup();
    rows.activeChasers = [chaser('c1')];
    watch.observe('chaser:terminate', 'c1');
    update({ activeChasers: [] });
    watch.chaseCompleted({ id: 'c1', symbol: 'BTCUSDT', size: 1, filled: 1 });
    vi.advanceTimersByTime(120_000);
    expect(told).toEqual([]);
  });

  it('a TWAP stop confirmed after the warning says so', () => {
    const { rows, told, watch, update } = setup();
    rows.activeTWAPs = [twap('t1', TwapStatus.Running)];
    watch.observe('twap:stop', 't1');
    vi.advanceTimersByTime(60_000);
    update({ activeTWAPs: [] });
    expect(told.at(-1)).toBe('TWAP stop is confirmed now.');
  });
});

describe('a start, resume or pause', () => {
  it('is never sent again, and says so when not confirmed', () => {
    const { rows, sent, told, watch } = setup();
    rows.activeTWAPs = [twap('t1', TwapStatus.Running), twap('t2', TwapStatus.Paused)];
    watch.observe('chaser:start', { symbol: 'BTCUSDT', side: OrderSide.Buy });
    watch.observe('twap:start', { symbol: 'ETHUSDT' });
    watch.observe('twap:pause', 't1');
    watch.observe('twap:resume', 't2');
    vi.advanceTimersByTime(60_000);
    expect(sent).toEqual([]);
    expect(told).toHaveLength(4);
    expect(told.every((message) => message.includes('not sent again'))).toBe(true);
  });

  it('is confirmed by a new row, or a chase that already finished', () => {
    const { told, watch, update } = setup();
    watch.observe('chaser:start', { symbol: 'BTCUSDT', side: OrderSide.Buy });
    update({ activeChasers: [chaser('c1')] });
    watch.observe('chaser:start', { symbol: 'ETHUSDT', side: OrderSide.Sell });
    watch.chaseCompleted({ symbol: 'ETHUSDT' });
    watch.observe('twap:start', { symbol: 'ETHUSDT' });
    update({ activeTWAPs: [twap('t1', TwapStatus.Running)] });
    vi.advanceTimersByTime(60_000);
    expect(told).toEqual([]);
  });

  it('a pause confirms on any status but running', () => {
    const { rows, told, watch, update } = setup();
    rows.activeTWAPs = [twap('t1', TwapStatus.Running)];
    watch.observe('twap:pause', 't1');
    update({ activeTWAPs: [twap('t1', TwapStatus.Paused)] });
    vi.advanceTimersByTime(60_000);
    expect(told).toEqual([]);
  });

  it('dispose ends every watch without a message', () => {
    const { rows, sent, told, watch, listeners } = setup();
    rows.activeChasers = [chaser('c1')];
    watch.observe('chaser:terminate', 'c1');
    watch.observe('twap:start', { symbol: 'ETHUSDT' });
    watch.dispose();
    vi.advanceTimersByTime(120_000);
    expect(sent).toEqual([]);
    expect(told).toEqual([]);
    expect(listeners.size).toBe(0);
  });
});
