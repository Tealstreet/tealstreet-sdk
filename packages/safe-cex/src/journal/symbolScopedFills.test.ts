import { readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';

import { describe, expect, it } from 'vitest';

import { canEnumerateFillSymbols, isSymbolScopedFills, SYMBOL_SCOPED_FILLS } from './symbolScopedFills';

/**
 * THE DRIFT GATE.
 *
 * `SYMBOL_SCOPED_FILLS` decides whether ingestion may ask a venue for fills
 * account-wide. If it disagrees with the source, the failure is silent: an
 * account-wide ask against a guarded venue returns `[]` with no request and no
 * error, and ingestion records a covered window over data it never fetched.
 *
 * So the registry is checked against the venues themselves rather than trusted.
 * This is the same class of gate as the journal axis table's, and it exists
 * because the `positions` axis DID drift — it declares three symbol-scoped
 * venues where the source has thirteen.
 */

const EXCHANGES_DIR = join(__dirname, '..', 'exchanges');

/** Venues whose `fetchHistoricalFills` opens with an `if (!symbol)` early return. */
const guardedVenuesFromSource = (): Set<string> => {
  const guarded = new Set<string>();

  for (const dir of readdirSync(EXCHANGES_DIR, { withFileTypes: true })) {
    if (!dir.isDirectory()) continue;

    for (const file of readdirSync(join(EXCHANGES_DIR, dir.name))) {
      if (!file.endsWith('.exchange.ts')) continue;

      const source = readFileSync(join(EXCHANGES_DIR, dir.name, file), 'utf8');
      const start = source.indexOf('fetchHistoricalFills = async');
      if (start === -1) continue;

      // Only the method's opening lines: a `!symbol` test deeper in the body is
      // a per-branch detail, not a refusal to answer account-wide.
      const head = source.slice(start, start + 700);
      if (/if\s*\(!symbol\)/.test(head)) guarded.add(file.replace('.exchange.ts', ''));
    }
  }

  // Robinhood Chain Lighter shares `lighter.exchange.ts`, so the filesystem
  // scan sees the implementation once while the registry must list both names.
  if (guarded.has('lighter')) {
    guarded.add('lighter-robinhood');
  }

  return guarded;
};

describe('SYMBOL_SCOPED_FILLS', () => {
  it('matches the venues that actually refuse an account-wide ask', () => {
    const fromSource = [...guardedVenuesFromSource()].sort();
    const fromRegistry = [...SYMBOL_SCOPED_FILLS].sort();

    // Listed both ways round so a failure names the venue rather than a diff of
    // two long arrays: a MISSING venue silently under-collects, an EXTRA one
    // needlessly forces the expensive per-symbol path.
    expect(fromSource.filter((v) => !SYMBOL_SCOPED_FILLS.has(v as never))).toEqual([]);
    expect(fromRegistry.filter((v) => !fromSource.includes(v))).toEqual([]);
  });

  it('finds the guard in the source at all, so an empty scan cannot pass vacuously', () => {
    expect(guardedVenuesFromSource().size).toBeGreaterThan(5);
  });

  it('treats an unlisted venue as answerable account-wide', () => {
    expect(isSymbolScopedFills('woo' as never)).toBe(false);
    expect(isSymbolScopedFills('binance' as never)).toBe(true);
  });
});

describe('canEnumerateFillSymbols', () => {
  // A symbol-scoped venue with no income ledger cannot be asked account-wide NOR
  // per symbol. That is `unsupported` — "we have no way to ask" — and must never
  // be reported as an empty account.
  it('is false without an income route', () => {
    expect(canEnumerateFillSymbols({})).toBe(false);
    expect(canEnumerateFillSymbols({ fetchIncomeHistoryRange: undefined })).toBe(false);
  });

  it('is true with one', () => {
    expect(canEnumerateFillSymbols({ fetchIncomeHistoryRange: () => [] })).toBe(true);
  });
});
