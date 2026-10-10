import type { Timeframe } from '../types';

import { timeframeToSeconds } from '../types';
import { getCandleStartTime } from './getCandleStartTime';
import { getNumCandlesInRange } from './getNumCandlesInRange';
import { normalizeEpochToSeconds } from './normalizeEpochToSeconds';

// ---------------------------------------------------------------------------
// The chart's weekly and monthly candles are CALENDAR candles.
//
// `getCandleStartTime` is an epoch floor, which is right for every intraday and
// daily candle and wrong for these two:
//
// - **1W starts on Monday 00:00 UTC.** TradingView builds W bars from daily
//   bars with its session's first day of week, which defaults to Monday, and
//   Binance / Bybit stamp their weekly klines Monday too. An epoch-floor week
//   starts on THURSDAY (1970-01-01 was one).
// - **1M starts on the 1st, 00:00 UTC.** safe-cex's `1M` is a nominal 30 days,
//   and a 30-day epoch floor lands on no month start at all. Binance `1M`,
//   Bybit `M` and OKX `1Mutc` all stamp the 1st 00:00 UTC.
//
// The footprint server speaks the same two periods (`timeframe` / `bar_period`
// `"1W"` | `"1M"`, llt/footprint/PROTOCOL.md), so a signal's `candleTs` and a
// merged bucket's `ts` equal the chart candle's own time.
//
// `getCandleStartTime` itself stays an epoch floor on purpose: it is public SDK
// surface (script-api-core re-exports it) and a venue that floors its weeks to
// the epoch would break if it moved. Call these where the candle is a CHART
// candle (the candle cache, the datafeed, a venue live bar built from trades).
// ---------------------------------------------------------------------------

export type CalendarPeriod = '1W' | '1M';

const DAY_MS = 86_400_000;
const WEEK_MS = 7 * DAY_MS;
/** 1970-01-05, the first Monday after the epoch. */
const FIRST_MONDAY_MS = 4 * DAY_MS;
/** Inputs above this are milliseconds, matching `getCandleStartTime`. */
const MS_THRESHOLD = 1_000_000_000_000;

/** The calendar period a chart interval draws (`'1w'` / `'1M'`), or `undefined` for an epoch-floor one. */
export function calendarPeriodForInterval(interval: string | undefined): CalendarPeriod | undefined {
  if (interval === '1w' || interval === '1W' || interval === 'W') return '1W';
  if (interval === '1M' || interval === 'M') return '1M';
  return undefined;
}

/**
 * The calendar period for a chart candle size in seconds. The chart's only
 * 604800 s and 2592000 s resolutions are its 1W and 1M, both calendar candles.
 */
export function calendarPeriodForChartSeconds(seconds: number): CalendarPeriod | undefined {
  if (seconds === 604_800) return '1W';
  if (seconds === 2_592_000) return '1M';
  return undefined;
}

/** Start (epoch ms) of the calendar candle holding `tsMs`, in UTC. */
export function calendarPeriodStartMs(period: CalendarPeriod, tsMs: number): number {
  if (period === '1W') return Math.floor((tsMs - FIRST_MONDAY_MS) / WEEK_MS) * WEEK_MS + FIRST_MONDAY_MS;
  const date = new Date(tsMs);
  return Date.UTC(date.getUTCFullYear(), date.getUTCMonth(), 1);
}

/** Start (epoch ms) of the calendar candle AFTER the one holding `tsMs`, in UTC. */
export function nextCalendarPeriodStartMs(period: CalendarPeriod, tsMs: number): number {
  const start = calendarPeriodStartMs(period, tsMs);
  if (period === '1W') return start + WEEK_MS;
  const date = new Date(start);
  return Date.UTC(date.getUTCFullYear(), date.getUTCMonth() + 1, 1);
}

/**
 * Drop-in for `getCandleStartTime` that knows the chart's calendar candles:
 * same units in and out (seconds, or ms when given ms), the epoch floor for
 * every other interval.
 */
export function chartCandleStartTime(interval: Timeframe | string, time: number): number {
  const period = calendarPeriodForInterval(interval);
  if (!period) return getCandleStartTime(interval as Timeframe, time);
  const startMs = calendarPeriodStartMs(period, normalizeEpochToSeconds(time) * 1000);
  return time > MS_THRESHOLD ? startMs : startMs / 1000;
}

/** Start of the chart candle after the one holding `time`, in the units given. */
export function nextChartCandleStartTime(interval: Timeframe | string, time: number): number {
  const period = calendarPeriodForInterval(interval);
  if (!period) {
    const seconds = timeframeToSeconds[interval as Timeframe];
    return getCandleStartTime(interval as Timeframe, time) + (time > MS_THRESHOLD ? seconds * 1000 : seconds);
  }
  const nextMs = nextCalendarPeriodStartMs(period, normalizeEpochToSeconds(time) * 1000);
  return time > MS_THRESHOLD ? nextMs : nextMs / 1000;
}

/**
 * When the candle that OPENED at `start` closes, in the units given — one
 * interval later, or one calendar month later for `1M`.
 *
 * Deliberately relative to the candle's own timestamp rather than to a chart
 * boundary, so it is right for a venue that stamps candles off the chart's
 * grid too: OKX `1D`/`1W` open at 16:00 UTC (Hong Kong midnight), and comparing
 * those against the UTC day or Monday-week start treats a still-forming candle
 * as closed for part of every period.
 */
export function chartCandleCloseTime(interval: Timeframe | string, start: number): number {
  const isMs = start > MS_THRESHOLD;
  if (calendarPeriodForInterval(interval) !== '1M') {
    const seconds =
      calendarPeriodForInterval(interval) === '1W' ? WEEK_MS / 1000 : timeframeToSeconds[interval as Timeframe];
    return start + (isMs ? seconds * 1000 : seconds);
  }
  const date = new Date(normalizeEpochToSeconds(start) * 1000);
  const closeMs = Date.UTC(
    date.getUTCFullYear(),
    date.getUTCMonth() + 1,
    date.getUTCDate(),
    date.getUTCHours(),
    date.getUTCMinutes(),
    date.getUTCSeconds(),
  );
  return isMs ? closeMs : closeMs / 1000;
}

/**
 * Calendar-aware `getNumCandlesInRange`: how many chart candles start in
 * `[from, to]`, both ends inclusive, in either unit. A nominal 30-day month
 * miscounts real ones — January to March is 59 days, which a 30-day division
 * calls two candles — and a venue `limit` that is one short drops the oldest.
 */
export function getNumChartCandlesInRange(interval: Timeframe | string, from: number, to: number): number {
  const period = calendarPeriodForInterval(interval);
  if (!period) return getNumCandlesInRange(interval as Timeframe, from, to);
  if (from > to) return 0;
  const fromMs = calendarPeriodStartMs(period, normalizeEpochToSeconds(from) * 1000);
  const toMs = calendarPeriodStartMs(period, normalizeEpochToSeconds(to) * 1000);
  if (period === '1W') return Math.round((toMs - fromMs) / WEEK_MS) + 1;
  const fromDate = new Date(fromMs);
  const toDate = new Date(toMs);
  return (
    (toDate.getUTCFullYear() - fromDate.getUTCFullYear()) * 12 + (toDate.getUTCMonth() - fromDate.getUTCMonth()) + 1
  );
}

/**
 * A per-subscription `seconds -> candle start (seconds)` for a live bar built
 * from trades. The epoch floor for intraday/daily intervals; for `1w`/`1M` the
 * calendar start, with the current bucket's bounds cached so a busy trade
 * stream does no `Date` work until the candle actually rolls over.
 */
export function createLiveCandleStart(interval: Timeframe | string): (timeSeconds: number) => number {
  const period = calendarPeriodForInterval(interval);
  if (!period) {
    const seconds = timeframeToSeconds[interval as Timeframe];
    return (timeSeconds) => Math.floor(timeSeconds / seconds) * seconds;
  }

  let start = Number.NaN;
  let end = Number.NaN;
  return (timeSeconds) => {
    if (timeSeconds >= start && timeSeconds < end) return start;
    const timeMs = timeSeconds * 1000;
    start = calendarPeriodStartMs(period, timeMs) / 1000;
    end = nextCalendarPeriodStartMs(period, timeMs) / 1000;
    return start;
  };
}
