import assert from 'node:assert/strict';
import { test } from 'node:test';

import { Frame, NONE, SECTION, TICKER_FIELDS, TickerState } from './wire.mjs';

const stringFrame = (strings) => {
  const encoder = new TextEncoder();
  const encoded = strings.map((text) => encoder.encode(text));
  const payloadBytes = 4 + (strings.length + 1) * 4 + encoded.reduce((n, bytes) => n + bytes.length, 0);
  const buffer = new ArrayBuffer(24 + payloadBytes);
  const header = new DataView(buffer);
  header.setUint32(0, 0x46584353, true);
  header.setUint16(4, 1, true);
  header.setUint16(6, 1, true);
  header.setUint16(16, SECTION.STRINGS, true);
  header.setUint32(20, payloadBytes, true);
  header.setUint32(24, strings.length, true);
  const offsets = new Uint32Array(buffer, 28, strings.length + 1);
  const bytes = new Uint8Array(buffer, 28 + offsets.byteLength);
  let at = 0;
  for (let i = 0; i < encoded.length; i++) {
    offsets[i] = at;
    bytes.set(encoded[i], at);
    at += encoded[i].length;
  }
  offsets[strings.length] = at;
  return new Frame(buffer);
};

test('unused strings do not reserve decode-cache slots', () => {
  const frame = stringFrame(Array.from({ length: 4096 }, (_, i) => `market-${i}`));
  assert.equal(frame.strings.cache.length, 0);
  assert.equal(frame.string(NONE), undefined);
  assert.equal(frame.strings.cache.length, 0);
  assert.equal(frame.string(0), 'market-0');
  assert.equal(frame.strings.cache.length, 1);
  assert.equal(frame.string(4095), 'market-4095');
});

test('lazy string decoding still caches empty and multibyte values', () => {
  const frame = stringFrame(['', 'é', 'SOL_USDC_PERP']);
  assert.equal(frame.string(0), '');
  assert.equal(frame.string(1), 'é');
  frame.strings.bytes.fill(0);
  assert.equal(frame.string(0), '');
  assert.equal(frame.string(1), 'é');
});

function settingsFrame(scalar, sides) {
  const count = scalar.length;
  const data = (4 + count * 8 + 7) & ~7;
  const length = data + count * 8 * (sides ? 3 : 1);
  const buffer = new ArrayBuffer(16 + 8 + length + 8 + 16);
  const view = new DataView(buffer);
  view.setUint32(0, 0x46584353, true);
  view.setUint16(4, 1, true);
  view.setUint16(6, 2, true);
  view.setUint16(16, SECTION.SETTINGS, true);
  view.setUint32(20, length, true);
  view.setUint32(24, count, true);
  for (let i = 0; i < count; i++) {
    view.setUint32(28 + i * 4, i, true);
    view.setUint32(28 + count * 4 + i * 4, 2, true);
    view.setFloat64(24 + data + i * 8, scalar[i], true);
    if (sides) {
      view.setFloat64(24 + data + count * 8 + i * 8, sides[0][i], true);
      view.setFloat64(24 + data + count * 16 + i * 8, sides[1][i], true);
    }
  }
  const status = 24 + length;
  view.setUint16(status, SECTION.STATUS, true);
  view.setUint32(status + 4, 16, true);
  view.setUint32(status + 8, 31, true);
  view.setFloat64(status + 16, 123, true);
  return buffer;
}

test('settings retain the scalar offset and independent side columns after transfer', () => {
  const frame = new Frame(structuredClone(settingsFrame([20, 15], [[20, NaN], [30, NaN]])));
  const settings = frame.settings();
  assert.deepEqual([...settings.leverage], [20, 15]);
  assert.deepEqual([...settings.buyLeverage], [20, NaN]);
  assert.deepEqual([...settings.sellLeverage], [30, NaN]);
  assert.equal(settings.marginType(0), 'cross');
  assert.equal(frame.status().latency, 123);
});

test('legacy settings fall back to scalar without reading the following section', () => {
  for (const values of [[12], [20, 15], []]) {
    const frame = new Frame(settingsFrame(values));
    const settings = frame.settings();
    assert.deepEqual([...settings.buyLeverage], values);
    assert.deepEqual([...settings.sellLeverage], values);
    assert.equal(frame.status().loaded, 31);
  }
});

function numericFrame(sections) {
  const buffer = new ArrayBuffer(16 + sections.reduce((size, section) => size + 8 + section.length, 0));
  const view = new DataView(buffer);
  view.setUint32(0, 0x46584353, true);
  view.setUint16(4, 1, true);
  view.setUint16(6, sections.length, true);
  let offset = 16;
  for (const section of sections) {
    view.setUint16(offset, section.kind, true);
    view.setUint32(offset + 4, section.length, true);
    section.write(view, offset + 8);
    offset += 8 + section.length;
  }
  return new Frame(structuredClone(buffer, { transfer: [buffer] }));
}

const tickerTable = (start) => ({
  kind: SECTION.TICKERS,
  length: 16 + TICKER_FIELDS.length * 16,
  write(view, offset) {
    view.setUint32(offset, 2, true);
    view.setUint32(offset + 4, 7, true);
    view.setUint32(offset + 8, 9, true);
    for (let i = 0; i < TICKER_FIELDS.length * 2; i++) view.setFloat64(offset + 16 + i * 8, start + i, true);
  },
});

const tickerDeltas = (values) => ({
  kind: SECTION.TICKER_DELTAS,
  length: values.length * 8,
  write(view, offset) {
    values.forEach((value, i) => view.setFloat64(offset + i * 8, value, true));
  },
});

test('ticker replacements adopt new columns before applying sparse multirow deltas', () => {
  const state = new TickerState();
  state.apply(numericFrame([tickerTable(100)]));
  const previousBid = state.columns.bid;
  state.apply(numericFrame([
    tickerTable(200),
    tickerDeltas([1, 1025, 301, 0, 0, 2, 302, 1, 0]),
  ]));
  assert.equal(state.count, 2);
  assert.deepEqual([...state.market], [7, 9]);
  assert.notEqual(state.columns.bid, previousBid);
  assert.deepEqual([...previousBid], [100, 101]);
  assert.deepEqual([...state.columns.bid], [200, 301]);
  assert.deepEqual([...state.columns.ask], [302, 203]);
  assert.deepEqual([...state.columns.quoteVolume], [220, 0]);
  for (const field of TICKER_FIELDS.slice(2, -1)) {
    const index = TICKER_FIELDS.indexOf(field);
    assert.deepEqual([...state.columns[field]], [200 + index * 2, 201 + index * 2]);
  }
});

test('ticker deltas respect replaced consumer columns and leave masked fields untouched', () => {
  const state = new TickerState();
  state.apply(numericFrame([tickerTable(100)]));
  const previousLast = state.columns.last;
  const last = new Float64Array([500, 501]);
  state.columns.last = last;
  state.apply(numericFrame([tickerDeltas([1, 4, 0, 0, 0])]));
  assert.equal(state.columns.last, last);
  assert.deepEqual([...last], [500, 0]);
  assert.deepEqual([...previousLast], [104, 105]);
  assert.deepEqual([...state.columns.bid], [100, 101]);
});

test('books preserve section order, empty sides and shared transferred row views', () => {
  const book = (market, bids, asks) => ({
    kind: SECTION.BOOK,
    length: 16 + (bids.length + asks.length) * 8,
    write(view, offset) {
      view.setUint32(offset, market, true);
      view.setUint32(offset + 4, bids.length / 3, true);
      view.setUint32(offset + 8, asks.length / 3, true);
      [...bids, ...asks].forEach((value, i) => view.setFloat64(offset + 16 + i * 8, value, true));
    },
  });
  const frame = numericFrame([
    book(7, [99, 2, 2, 98, 3, 5], [100, 4, 4]),
    { kind: SECTION.STATUS, length: 16, write(view, offset) { view.setUint32(offset, 31, true); } },
    book(9, [], [200, 1, 1]),
    book(10, [], []),
  ]);
  const books = [...frame.books()];
  assert.deepEqual(books.map(({ market, bids, asks }) => [market, [...bids], [...asks]]), [
    [7, [99, 2, 2, 98, 3, 5], [100, 4, 4]],
    [9, [], [200, 1, 1]],
    [10, [], []],
  ]);
  assert.equal(frame.status().loaded, 31);
  for (const { bids, asks } of books) {
    assert.equal(bids.buffer, frame.buffer);
    assert.equal(asks.buffer, frame.buffer);
  }
  books[0].bids[1] = 6;
  assert.equal(frame.books().next().value.bids[1], 6);
});

function positionFrame(withIds) {
  const strings = stringFrame(['BTCUSDC', 'BTCUSDC:isolated']).buffer;
  const prefix = (strings.byteLength + 7) & ~7;
  const count = 2, data = 24, length = data + 9 * count * 8 + (withIds ? 8 : 0);
  const buffer = new ArrayBuffer(prefix + 8 + length + 24);
  new Uint8Array(buffer).set(new Uint8Array(strings));
  const view = new DataView(buffer);
  view.setUint16(6, 3, true);
  view.setUint32(20, prefix - 24, true);
  view.setUint16(prefix, SECTION.POSITIONS, true);
  view.setUint32(prefix + 4, length, true);
  view.setUint32(prefix + 8, count, true);
  if (withIds) {
    view.setUint32(prefix + 8 + data + 144, 0, true);
    view.setUint32(prefix + 8 + data + 148, 1, true);
  }
  const status = prefix + 8 + length;
  view.setUint16(status, SECTION.STATUS, true);
  view.setUint32(status + 4, 16, true);
  view.setUint32(status + 8, 31, true);
  return new Frame(structuredClone(buffer));
}

test('position IDs survive transfer and legacy frames never read the next section as IDs', () => {
  const frame = positionFrame(true);
  assert.equal(frame.positions().id(0), 'BTCUSDC');
  assert.equal(frame.positions().id(1), 'BTCUSDC:isolated');
  assert.equal(frame.status().loaded, 31);
  const legacy = positionFrame(false);
  assert.equal(legacy.positions().id(0), undefined);
  assert.equal(legacy.positions().id(1), undefined);
  assert.equal(legacy.status().loaded, 31);
});

function balanceFrame(coins, currencies) {
  const currencyRows = Object.entries(currencies ?? {});
  const strings = [...(coins?.map((c) => c.base) ?? []), ...currencyRows.map(([currency]) => currency)];
  const stringBuffer = stringFrame(strings).buffer;
  const count = coins?.length ?? 0;
  const at = (36 + count * 4 + 7) & ~7;
  const coinLength = coins || currencies ? at + count * 16 : 32;
  const currencyAt = (coinLength + 4 + currencyRows.length * 4 + 7) & ~7;
  const length = currencies ? currencyAt + currencyRows.length * 32 : coinLength;
  const result = new ArrayBuffer(16 + 8 + length + stringBuffer.byteLength - 16);
  const view = new DataView(result);
  new Uint8Array(result).set(new Uint8Array(stringBuffer, 0, 16));
  view.setUint16(6, 2, true);
  view.setUint16(16, SECTION.BALANCE, true);
  view.setUint32(20, length, true);
  [1, 2, 3, 4].forEach((v, i) => view.setFloat64(24 + i * 8, v, true));
  if (coins || currencies) {
    view.setUint32(56, count, true);
    (coins ?? []).forEach((coin, i) => {
      view.setUint32(60 + i * 4, i, true);
      view.setFloat64(24 + at + i * 8, coin.amount, true);
      view.setFloat64(24 + at + count * 8 + i * 8, coin.notional, true);
    });
  }
  if (currencies) {
    view.setUint32(24 + coinLength, currencyRows.length, true);
    currencyRows.forEach(([, values], i) => {
      view.setUint32(24 + coinLength + 4 + i * 4, count + i, true);
      ['used', 'free', 'total', 'upnl'].forEach((field, column) => {
        view.setFloat64(24 + currencyAt + column * currencyRows.length * 8 + i * 8, values[field], true);
      });
    });
  }
  new Uint8Array(result, 24 + length).set(new Uint8Array(stringBuffer, 16));
  return result;
}

test('optional collateral columns survive buffer transfer and distinguish absent from empty', () => {
  assert.deepEqual(new Frame(balanceFrame()).balance(), { used: 1, free: 2, total: 3, upnl: 4 });
  assert.deepEqual(new Frame(balanceFrame([])).balance().coins, {});
  const coins = [{ base: 'BTC', amount: 0.1, notional: 5000 }, { base: 'ETH', amount: 2, notional: 6000 }];
  const buffer = balanceFrame(coins);
  const transferred = structuredClone(buffer, { transfer: [buffer] });
  assert.equal(buffer.byteLength, 0);
  assert.deepEqual(new Frame(transferred).balance().coins, { BTC: coins[0], ETH: coins[1] });
});

test('optional currency balances preserve full values, coexist with coins, and clear explicitly', () => {
  const currencies = {
    USDT: { used: 1.25, free: 8.75, total: 10, upnl: -3 },
    USDC: { used: 2, free: 4, total: 6, upnl: 0.5 },
  };
  assert.equal(new Frame(balanceFrame()).balance().currencies, undefined);
  assert.equal(new Frame(balanceFrame([])).balance().currencies, undefined);
  assert.deepEqual(new Frame(balanceFrame(undefined, {})).balance(), { used: 1, free: 2, total: 3, upnl: 4, coins: {}, currencies: {} });
  const currencyOnly = new Frame(balanceFrame(undefined, currencies)).balance();
  assert.deepEqual(currencyOnly.currencies, currencies);
  assert.deepEqual(currencyOnly.coins, {});
  const coins = [{ base: 'BTC', amount: 0.1, notional: 5000 }];
  const buffer = balanceFrame(coins, currencies);
  const transferred = structuredClone(buffer, { transfer: [buffer] });
  assert.equal(buffer.byteLength, 0);
  const balance = new Frame(transferred).balance();
  assert.deepEqual(balance.coins, { BTC: coins[0] });
  assert.deepEqual(balance.currencies, currencies);
  const unusual = Object.fromEntries([['__proto__', currencies.USDT]]);
  const decoded = new Frame(balanceFrame(undefined, unusual)).balance().currencies;
  assert.deepEqual(Object.keys(decoded), ['__proto__']);
  assert.deepEqual(decoded.__proto__, currencies.USDT);
  assert.equal(Object.getPrototypeOf(decoded), Object.prototype);
});

function tickerFrame(identities) {
  const count = 2;
  const strings = ['LIVEUSDT', 'OLDUSDT', 'native-retired-id'];
  const stringBuffer = stringFrame(strings).buffer;
  const at = (4 + count * 4 + 7) & ~7;
  const tail = at + count * TICKER_FIELDS.length * 8;
  const length = tail + (identities ? count * 8 : 0);
  const result = new ArrayBuffer(24 + length + stringBuffer.byteLength - 16);
  const view = new DataView(result);
  new Uint8Array(result).set(new Uint8Array(stringBuffer, 0, 16));
  view.setUint16(6, 2, true);
  view.setUint16(16, SECTION.TICKERS, true);
  view.setUint32(20, length, true);
  view.setUint32(24, count, true);
  view.setUint32(28, 0, true);
  view.setUint32(32, identities ? NONE : 1, true);
  TICKER_FIELDS.forEach((_, column) => {
    view.setFloat64(24 + at + column * count * 8, column + 1, true);
    view.setFloat64(24 + at + column * count * 8 + 8, column + 100, true);
  });
  if (identities) {
    [0, 1, 0, 2].forEach((reference, i) => view.setUint32(24 + tail + i * 4, reference, true));
  }
  new Uint8Array(result, 24 + length).set(new Uint8Array(stringBuffer, 16));
  return result;
}

test('orphan ticker identities survive transfer and packed deltas without changing legacy tables', () => {
  const legacy = new Frame(tickerFrame(false)).tickers();
  assert.equal(legacy.identity, undefined);
  assert.deepEqual([...legacy.last], [3, 102]);
  const buffer = tickerFrame(true);
  const frame = new Frame(structuredClone(buffer, { transfer: [buffer] }));
  assert.equal(buffer.byteLength, 0);
  const tickers = frame.tickers();
  assert.deepEqual([...tickers.market], [0, NONE]);
  assert.deepEqual(tickers.identity(0), { symbol: 'LIVEUSDT', id: 'LIVEUSDT' });
  assert.deepEqual(tickers.identity(1), { symbol: 'OLDUSDT', id: 'native-retired-id' });
  assert.deepEqual([...tickers.last], [3, 102]);
  const state = new TickerState();
  state.apply(frame);
  state.apply({ tickers: () => undefined, tickerDeltas: () => Float64Array.of(1, 4, 500) });
  assert.equal(state.columns.last[1], 500);
  assert.deepEqual(state.identity(1), { symbol: 'OLDUSDT', id: 'native-retired-id' });
  state.apply(new Frame(tickerFrame(false)));
  assert.equal(state.identity, undefined, 'a later legacy replacement clears the old identity closure');
});

const bookSection = (kind, market, bids = [], asks = []) => ({
  kind,
  length: 16 + (bids.length + asks.length) * 8,
  write(view, offset) {
    const width = kind === SECTION.BOOK ? 3 : 2;
    view.setUint32(offset, market, true);
    view.setUint32(offset + 4, bids.length / width, true);
    view.setUint32(offset + 8, asks.length / width, true);
    [...bids, ...asks].forEach((value, i) => view.setFloat64(offset + 16 + i * 8, value, true));
  },
});

test('book deltas expose transferred zero-copy pairs independently of full reset books', () => {
  const frame = numericFrame([
    bookSection(SECTION.BOOK, 7, [100, 2, 2], [101, 3, 3]),
    bookSection(SECTION.BOOK_DELTA, 9, [50, 0, 49, 7], [51, 8]),
    bookSection(SECTION.BOOK_DELTA, 7),
  ]);
  const [reset] = frame.books();
  assert.deepEqual([...reset.bids], [100, 2, 2]);
  assert.deepEqual([...reset.asks], [101, 3, 3]);
  const [delta, empty] = frame.bookDeltas();
  assert.equal(delta.market, 9);
  assert.deepEqual([...delta.bids], [50, 0, 49, 7]);
  assert.deepEqual([...delta.asks], [51, 8]);
  assert.equal(delta.bids.buffer, frame.buffer);
  assert.equal(delta.asks.buffer, frame.buffer);
  assert.equal(empty.market, 7);
  assert.equal(empty.bids.length, 0);
  assert.equal(empty.asks.length, 0);
});

test('book readers refuse short, overflowing, trailing and reserved payloads', () => {
  for (const kind of [SECTION.BOOK, SECTION.BOOK_DELTA]) {
    for (const mutate of [
      (view) => view.setUint32(20, 8, true),
      (view) => view.setUint32(28, 0xffffffff, true),
      (view) => view.setUint32(32, 1, true),
      (view) => view.setUint32(36, 1, true),
    ]) {
      const frame = numericFrame([bookSection(kind, 0), bookSection(SECTION.BOOK, 1)]);
      mutate(new DataView(frame.buffer));
      assert.throws(() => {
        const mutated = new Frame(frame.buffer);
        return [...(kind === SECTION.BOOK ? mutated.books() : mutated.bookDeltas())];
      }, /invalid book|truncated|trailing/);
    }
  }
  const frame = numericFrame([bookSection(SECTION.BOOK_DELTA, 0, [10, 1])]);
  new DataView(frame.buffer).setUint32(28, 0, true);
  assert.throws(() => [...frame.bookDeltas()], /invalid book/);
});

test('frame envelope refuses truncated section headers, payloads and trailing bytes', () => {
  assert.throws(() => new Frame(new ArrayBuffer(8)), /truncated/);
  const buffer = numericFrame([bookSection(SECTION.BOOK_DELTA, 0)]).buffer;
  assert.throws(() => new Frame(buffer.slice(0, 20)), /truncated/);
  assert.throws(() => new Frame(buffer.slice(0, buffer.byteLength - 1)), /truncated/);
  const extra = new Uint8Array(buffer.byteLength + 8);
  extra.set(new Uint8Array(buffer));
  assert.throws(() => new Frame(extra.buffer), /trailing/);
});
