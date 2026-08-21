import type { ExchangeName } from '../types';

/**
 * Venues whose trading is not real, and which must therefore never be ingested.
 *
 * Their API answers are genuine — real endpoints, real ids, real fills and
 * orders — describing trades that never happened with money. Nothing in
 * `historical_*` carries a "this was paper" flag, and adding one is worse than
 * excluding them: every consumer would then have to remember to filter, and the
 * first one that forgets reports paper volume as real. So the exclusion lives
 * here, at the point of collection, and the tables stay unambiguous.
 *
 * **This was a test-only list until 2026-08-17, and that is exactly how it
 * failed.** The drift test in
 * `__tests__/audit-regressions/exchange-metadata-capabilities.test.ts` carried
 * its own copy to exempt these venues from the fills registry, so the fills axis
 * skipped them for the wrong reason and the ORDERS axis — which gates purely on
 * the capability brand, with no venue registry at all — collected them happily.
 * 765 bybitdemo orders across 7 users reached production before anyone looked at
 * the table. A rule enforced only in a test is not enforced.
 *
 * The test now imports this list rather than redeclaring it, so the runtime gate
 * and the gate that polices it cannot drift apart.
 */
/**
 * DUPLICATED IN SQL. `fn_admin_open_positions` in
 * `supabase/admin_user_metrics.sql` excludes the same venues by name, because it
 * reads position snapshots directly and cannot import this file. Add a venue
 * here and it must be added there too — otherwise paper positions get ranked
 * against real money on the admin dashboard, which is exactly what `bybitdemo`
 * was doing when it held the second-largest position in the table.
 */
export const PRACTICE_VENUES: ReadonlySet<ExchangeName> = new Set([
  'bybitdemo',
  'bitmextestnet',
  'hyrotrader',
  'okxdemo',
] as ExchangeName[]);

/**
 * Whether this venue's trading is paper.
 *
 * Checked for EVERY axis, ahead of any capability test. Capability answers "can
 * this venue serve the data", which is emphatically true for a demo venue — that
 * is the whole point of one — so a capability-shaped gate can never exclude it.
 */
export const isPracticeVenue = (exchange: ExchangeName): boolean => PRACTICE_VENUES.has(exchange);
