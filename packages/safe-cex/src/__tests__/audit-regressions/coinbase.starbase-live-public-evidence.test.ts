import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

import {
  normalizeCoinbaseStarbaseInstrument,
  normalizeCoinbaseStarbaseOrderBookSnapshot,
  normalizeCoinbaseStarbaseTicker,
} from '../reference/coinbase/coinbase.starbase-public';
import {
  STARBASE_BOOK_CHANGE_PAYLOAD,
  STARBASE_BOOK_SNAPSHOT_PAYLOAD,
  STARBASE_PUBLIC_ETH_INSTRUMENT_FIXTURE,
  STARBASE_PUBLIC_INSTRUMENT_FIXTURE,
  STARBASE_PUBLIC_ORDER_BOOK_FIXTURE,
  STARBASE_PUBLIC_TICKER_FIXTURE,
} from './coinbase.starbase-public.fixtures';

const EVIDENCE_DIRECTORY = resolve(__dirname, 'coinbase.starbase-live-public.evidence');

const EVIDENCE_HASHES = {
  'instruments.json': '419b0d726831bfe133bfc69519bfd01dcef71595bfb9853f2d24489fe9710fef',
  'public-trades.json': 'edb8ea3917d079a299a40708ac9e3eb41abfa3228b38cced294bda5cca5a4ac0',
  'rest-book.json': '48752ba2187b34eabdbf41da71451260f4b8a970c77bb01451c4e740efefdeeb',
  'ticker.json': 'f04333105163859f5d6ca6537a981ae5051df186f95998e5143d13053cce4a9e',
  'ws-book-frames.json': '2022540e14b650fbeae2f33eb0710a459e4ef16124960e1bb25e358894d98941',
} as const;

const readEvidenceBytes = (name: keyof typeof EVIDENCE_HASHES): Buffer =>
  readFileSync(resolve(EVIDENCE_DIRECTORY, name));

const readEvidenceJson = <Result>(name: keyof typeof EVIDENCE_HASHES): Result =>
  JSON.parse(readEvidenceBytes(name).toString('utf8')) as Result;

describe('Coinbase Starbase committed live-public evidence', () => {
  it('pins the SHA-256 of the exact committed sanitized bytes', () => {
    for (const [name, expectedHash] of Object.entries(EVIDENCE_HASHES)) {
      expect(createHash('sha256').update(readEvidenceBytes(name as keyof typeof EVIDENCE_HASHES)).digest('hex')).toBe(
        expectedHash,
      );
    }
  });

  it('derives instrument discriminators and normalized BTC/ETH contracts from the committed capture', () => {
    const evidence = readEvidenceJson<{ results: Array<Record<string, unknown>> }>('instruments.json');
    expect(evidence.results).toHaveLength(2);
    for (const result of evidence.results) {
      expect(result).toMatchObject({
        future_type: 'linear',
        instrument_type: 'linear',
        underlying_type: 'crypto',
      });
      expect(Object.prototype.hasOwnProperty.call(result, 'option_type')).toBe(false);
    }
    expect(evidence.results.map(normalizeCoinbaseStarbaseInstrument)).toEqual([
      normalizeCoinbaseStarbaseInstrument(STARBASE_PUBLIC_INSTRUMENT_FIXTURE),
      normalizeCoinbaseStarbaseInstrument(STARBASE_PUBLIC_ETH_INSTRUMENT_FIXTURE),
    ]);
  });

  it('derives the REST book and ticker values used by deterministic mapper fixtures', () => {
    const bookEvidence = readEvidenceJson<{ result: { asks: number[][]; bids: number[][] } }>('rest-book.json');
    expect(bookEvidence.result.asks.slice(0, 2)).toEqual(STARBASE_PUBLIC_ORDER_BOOK_FIXTURE.asks);
    expect(bookEvidence.result.bids.slice(0, 2)).toEqual(STARBASE_PUBLIC_ORDER_BOOK_FIXTURE.bids);
    expect(normalizeCoinbaseStarbaseOrderBookSnapshot(bookEvidence.result).asks[0]?.amount).toBe(0.0002);

    const tickerEvidence = readEvidenceJson<{ result: Record<string, unknown> }>('ticker.json');
    expect(tickerEvidence.result).toMatchObject(STARBASE_PUBLIC_TICKER_FIXTURE);
    expect(normalizeCoinbaseStarbaseTicker(tickerEvidence.result)).toEqual(
      normalizeCoinbaseStarbaseTicker(STARBASE_PUBLIC_TICKER_FIXTURE),
    );
  });

  it('corroborates public trade amount arithmetic for both supported products', () => {
    const evidence = readEvidenceJson<{
      results: Record<string, { contract_size: number; trades: Array<{ amount: number; contracts: number }> }>;
    }>('public-trades.json');

    for (const product of Object.values(evidence.results)) {
      for (const trade of product.trades) {
        expect(trade.amount).toBeCloseTo(trade.contracts * product.contract_size, 12);
      }
    }
  });

  it('derives the deterministic WebSocket snapshot/change payload bytes', () => {
    const evidence = readEvidenceJson<{ frames: Array<{ data: Record<string, unknown> }> }>('ws-book-frames.json');
    expect(evidence.frames.map(({ data }) => ({ data }))).toEqual([
      STARBASE_BOOK_SNAPSHOT_PAYLOAD,
      STARBASE_BOOK_CHANGE_PAYLOAD,
    ]);
  });
});
