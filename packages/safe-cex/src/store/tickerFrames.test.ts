import { expect, it } from 'vitest';

import { Frame } from '@tealstreet/safe-cex-wasm/wire';
import type { Ticker } from '../generated/Ticker';
import { getTickerPredictedBestAsk } from '../utils/getTickerPredictedBestAsk';
import { DefaultStore } from './store.base';
import { readTickerFrame, readTickerDeltaFrame, readTickerObservationFrame } from './tickerFrames';

const observationFrame = (times: number[], row = 0) => {
  const bytes = new ArrayBuffer(56);
  const view = new DataView(bytes);
  new Uint8Array(bytes, 0, 4).set([83, 67, 88, 70]);
  view.setUint16(4, 1, true);
  view.setUint16(6, 1, true);
  view.setFloat64(8, 99_000, true);
  view.setUint16(16, 24, true);
  view.setUint32(20, 32, true);
  [row, ...times].forEach((value, slot) => view.setFloat64(24 + slot * 8, value, true));
  return new Frame(bytes);
};

const ticker: Ticker = {
  id: 'BTC', symbol: 'BTC-PERP', bid: 0, ask: 0, last: 3, mark: 4, index: 0,
  percentage: 0, openInterest: 0, fundingRate: 0, fundingTime: 0, volume: 0, quoteVolume: 0,
};

it('observation-only packed frames keep independent source times through store serialization and worker objects', () => {
  const store = new DefaultStore();
  store.update({ tickers: [ticker] });
  const frame = observationFrame([0, -Infinity, 12.5]);
  store.update({ tickerObservations: readTickerObservationFrame(frame, store.tickers) });
  expect(store.tickers[0].observedAt).toEqual([0, null, 12.5]);
  const worker = new DefaultStore();
  worker.update(structuredClone(store.serialize()), true);
  expect(worker.tickers[0].observedAt).toEqual([0, null, 12.5]);
  expect(worker.tickers[0].last).toBe(3);
  expect(getTickerPredictedBestAsk(worker.tickers[0])).toBe(3);
  store.update({ tickers: [{ ...store.tickers[0] }] });
  expect(store.tickers[0].observedAt).toEqual([0, null, 12.5]);
});

it('a missing observation section preserves provenance and an invalid row refuses', () => {
  const store = new DefaultStore();
  store.update({ tickers: [{ ...ticker, observedAt: [1, null, 3] }] });
  const bytes = new ArrayBuffer(16);
  new Uint8Array(bytes, 0, 4).set([83, 67, 88, 70]);
  new DataView(bytes).setUint16(4, 1, true);
  const empty = new Frame(bytes);
  expect(readTickerFrame(empty.tickers(), () => undefined)).toBeUndefined();
  expect(readTickerDeltaFrame(empty, 1)).toBeUndefined();
  expect(readTickerObservationFrame(empty, store.tickers)).toBeUndefined();
  expect(() => readTickerObservationFrame(observationFrame([1, 2, 3], 1), store.tickers)).toThrow('Invalid ticker observation row');
  expect(store.tickers[0].observedAt).toEqual([1, null, 3]);
});

it('native snapshots and midpoint clears decode into the generated contract without restamping observations', async () => {
  const { execFileSync } = await import('node:child_process');
  const { resolve } = await import('node:path');
  const manifest = resolve(__dirname, '../../../../sdk-rs/Cargo.toml');
  const frames: number[][] = JSON.parse(execFileSync('cargo', [
    'run', '--locked', '--manifest-path', manifest, '-p', 'safe-cex-engine',
    '--example', 'ticker_contract_frames', '-j2', '--quiet',
  ], { encoding: 'utf8', timeout: 120_000 }));
  const store = new DefaultStore();
  const apply = (bytes: number[]) => {
    const frame = new Frame(Uint8Array.from(bytes).buffer);
    const tickers = readTickerFrame(frame.tickers(), (index) => index === 0 ? { id: 'BTC', symbol: 'BTC-PERP' } : undefined);
    store.update({
      ...(tickers && { tickers }),
      tickerDeltas: readTickerDeltaFrame(frame, tickers?.length ?? store.tickers.length, Boolean(tickers)),
      tickerObservations: readTickerObservationFrame(frame, tickers ?? store.tickers),
    });
  };
  const table = new Frame(Uint8Array.from(frames[0]).buffer).tickers();
  if (!table) throw new Error('Native fixture is missing its ticker table');
  expect(() => readTickerFrame({ ...table, identity: undefined }, () => undefined)).toThrow('Missing native ticker identity');
  expect(readTickerDeltaFrame(new Frame(Uint8Array.from(frames[1]).buffer), 2, true)).toBeUndefined();
  apply(frames[0]);
  expect(store.tickers[0]).toMatchObject({ bid: 0, ask: 2, last: 3, mark: 4, index: 5, midPrice: 0, observedAt: [0, null, 12.5] });
  expect(store.tickers[1]).toMatchObject({ id: 'native-orphan-id', symbol: 'DELISTED', last: 6 });
  const identity = store.tickers[0];
  apply(frames[1]);
  expect(store.tickers[0]).toBe(identity);
  expect(store.tickers[0]).not.toHaveProperty('midPrice');
  expect(store.tickers[0].observedAt).toEqual([0, null, 12.5]);
  apply(frames[2]);
  expect(store.tickers[0]).not.toHaveProperty('midPrice');
  expect(store.tickers[0].observedAt).toEqual([0, null, 12.5]);
  const worker = new DefaultStore();
  worker.update(structuredClone(store.serialize()), true);
  expect(worker.tickers[0]).toEqual(store.tickers[0]);
  expect(getTickerPredictedBestAsk(worker.tickers[0])).toBe(2);
}, 120_000);
