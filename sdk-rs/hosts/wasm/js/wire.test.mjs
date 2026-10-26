import assert from 'node:assert/strict';
import { test } from 'node:test';

import { Frame, NONE, SECTION, TICKER_FIELDS, TickerState, marketQuantityUnits, orderEnums, positionEnums } from './wire.mjs';

test('account read markers survive transfer and do not change legacy frames', () => {
  const buffer = new ArrayBuffer(24);
  const view = new DataView(buffer);
  view.setUint32(0, 0x46584353, true);
  view.setUint16(4, 1, true);
  view.setUint16(6, 1, true);
  view.setUint16(16, SECTION.ACCOUNT_READ, true);
  assert.equal(new Frame(structuredClone(buffer)).accountRead(), true);
  view.setUint16(16, SECTION.STATUS, true);
  assert.equal(new Frame(buffer).accountRead(), false);
  const invalid = new ArrayBuffer(32);
  new Uint8Array(invalid).set(new Uint8Array(buffer));
  const bad = new DataView(invalid);
  bad.setUint16(16, SECTION.ACCOUNT_READ, true);
  bad.setUint32(20, 8, true);
  assert.throws(() => new Frame(invalid).accountRead(), /Invalid account read marker/);
});

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
    view.setUint16(offset + 2, section.revision ?? 0, true);
    view.setUint32(offset + 4, section.length, true);
    section.write(view, offset + 8);
    offset += 8 + section.length;
  }
  return new Frame(structuredClone(buffer, { transfer: [buffer] }));
}

const tickerTable = (start) => ({
  kind: SECTION.TICKERS,
  length: 16 + 11 * 16,
  write(view, offset) {
    view.setUint32(offset, 2, true);
    view.setUint32(offset + 4, 7, true);
    view.setUint32(offset + 8, 9, true);
    for (let i = 0; i < 11 * 2; i++) view.setFloat64(offset + 16 + i * 8, start + i, true);
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
  for (const field of TICKER_FIELDS.slice(2, 10)) {
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

test('ticker bit traversal preserves dense and sparse field order, NaN, signed zero and consumer columns', () => {
  const state = new TickerState();
  state.apply(numericFrame([tickerTable(100)]));
  const bid = Float64Array.of(600, 601);
  state.columns.bid = bid;
  const previousLast = state.columns.last;
  delete state.columns.fundingRate;
  const dense = TICKER_FIELDS.map((_, index) => index === 0 ? -0 : index === 2 ? NaN : index + 10);
  const delta = tickerDeltas([
    0, (1 << TICKER_FIELDS.length) - 1, ...dense,
    1, (1 << 0) | (1 << 7) | (1 << 11), 0, -0, NaN,
    0, 0,
  ]);
  delta.revision = 1;
  state.apply(numericFrame([delta]));
  for (const [index, field] of TICKER_FIELDS.entries()) assert.ok(Object.is(state.columns[field][0], dense[index]), field);
  assert.equal(state.columns.bid, bid);
  assert.ok(Object.is(bid[0], -0));
  assert.ok(Object.is(bid[1], 0));
  assert.equal(state.columns.last, previousLast);
  assert.equal(state.columns.last[1], 105);
  assert.ok(Object.is(state.columns.fundingRate[1], -0));
  assert.ok(Number.isNaN(state.columns.midPrice[1]));

  // A highest-field-only update initializes absent rows with NaN; a zero mask creates no columns.
  const sparse = new TickerState();
  sparse.apply(numericFrame([tickerTable(100)]));
  const zero = tickerDeltas([1, 0]);
  zero.revision = 1;
  sparse.apply(numericFrame([zero]));
  assert.equal(sparse.columns.midPrice, undefined);
  const highest = tickerDeltas([1, 1 << 11, -0]);
  highest.revision = 1;
  sparse.apply(numericFrame([highest]));
  assert.ok(Number.isNaN(sparse.columns.midPrice[0]));
  assert.ok(Object.is(sparse.columns.midPrice[1], -0));
});

test('ticker decoder validates malformed later rows before any delta mutates columns', () => {
  for (const tail of [[1, 1], [1, 1 << TICKER_FIELDS.length, 500]]) {
    const state = new TickerState();
    state.apply(numericFrame([tickerTable(100)]));
    const bid = state.columns.bid;
    const delta = tickerDeltas([0, 1, 700, ...tail]);
    delta.revision = 1;
    assert.throws(() => state.apply(numericFrame([delta])), /ticker delta/);
    assert.equal(state.columns.bid, bid);
    assert.deepEqual([...bid], [100, 101]);
  }
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
  const coinsAbsent = coins === undefined;
  const currencyRows = Array.isArray(currencies)
    ? currencies.map(({ currency, ...balance }) => [currency, balance]) : Object.entries(currencies ?? {});
  if (currencies) coins ??= [];
  const strings = [...(coins?.map((c) => c.base) ?? []), ...currencyRows.map(([currency]) => currency)];
  const stringBuffer = stringFrame(strings).buffer;
  const count = coins?.length ?? 0;
  const at = (36 + count * 4 + 7) & ~7;
  const tail = at + count * 16;
  const currencyAt = (tail + 8 + currencyRows.length * 4 + 7) & ~7;
  const length = currencies ? currencyAt + currencyRows.length * 32 : coins ? tail : 32;
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
    view.setUint32(24 + tail, currencyRows.length, true);
    view.setUint32(24 + tail + 4, Number(coinsAbsent), true);
    currencyRows.forEach(([, balance], i) => {
      view.setUint32(24 + tail + 8 + i * 4, count + i, true);
      ['total', 'free', 'used', 'upnl'].forEach((field, column) => {
        view.setFloat64(24 + currencyAt + (column * currencyRows.length + i) * 8, balance[field], true);
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

test('prediction metadata is optional, lossless and isolated from following sections', () => {
  const strings = stringFrame(['0xcondition', 'Yes', 'Will café open?', 'Exact description', '2026-12-31T00:00:00Z', 'cafe-open']);
  const data = numericFrame([
    { kind: SECTION.PREDICTION_MARKETS, length: 48, write(view, offset) {
      view.setUint32(offset, 1, true);
      view.setUint32(offset + 4, 7, true);
      for (let i = 0; i < 6; i++) view.setUint32(offset + 8 + i * 4, i, true);
      view.setUint32(offset + 32, 1, true);
      view.setFloat64(offset + 40, 0.001, true);
    } },
    { kind: SECTION.PREDICTION_HOLDINGS, length: 24, write(view, offset) {
      view.setUint32(offset, 2, true);
      view.setUint32(offset + 4, 7, true);
      view.setUint32(offset + 8, 9, true);
      view.setUint32(offset + 12, 1, true);
      view.setUint32(offset + 16, 2, true);
    } },
  ]);
  data.strings = strings.strings;
  const markets = data.predictionMarkets();
  assert.deepEqual([...markets.market], [7]);
  assert.deepEqual([...markets.flags], [1]);
  assert.deepEqual([...markets.tickSize], [0.001]);
  assert.equal(markets.conditionId(0), '0xcondition');
  assert.equal(markets.outcome(0), 'Yes');
  assert.equal(markets.question(0), 'Will café open?');
  assert.equal(markets.description(0), 'Exact description');
  assert.equal(markets.endDate(0), '2026-12-31T00:00:00Z');
  assert.equal(markets.slug(0), 'cafe-open');
  assert.deepEqual([...data.predictionHoldings().market], [7, 9]);
  assert.deepEqual([...data.predictionHoldings().flags], [1, 2]);
  assert.equal(strings.predictionMarkets(), undefined);
  assert.equal(strings.predictionHoldings(), undefined);
  const empty = numericFrame([
    { kind: SECTION.PREDICTION_MARKETS, length: 8, write(view, offset) { view.setUint32(offset, 0, true); } },
    { kind: SECTION.PREDICTION_HOLDINGS, length: 8, write(view, offset) { view.setUint32(offset, 0, true); } },
  ]);
  assert.equal(empty.predictionMarkets().count, 0);
  assert.equal(empty.predictionHoldings().count, 0);
});

test('spot currency columns retain reserved quantities and explicitly clear an empty snapshot', () => {
  const coins = [{ base: 'BTC', amount: 0.1, notional: 5000 }];
  const currencies = [{ currency: 'BTC', total: 2, free: 1.25, used: 0.75, upnl: 0 },
    { currency: '__proto__', total: 3, free: 0, used: 3, upnl: -1 }];
  const source = balanceFrame(coins, currencies);
  const result = new Frame(structuredClone(source, { transfer: [source] })).balance();
  assert.deepEqual(result.coins, { BTC: coins[0] });
  assert.deepEqual(result.currencies.BTC, { total: 2, free: 1.25, used: 0.75, upnl: 0 });
  assert.equal(Object.getPrototypeOf(result.currencies), Object.prototype);
  assert.deepEqual(Object.getOwnPropertyDescriptor(result.currencies, '__proto__').value,
    { total: 3, free: 0, used: 3, upnl: -1 });
  assert.deepEqual(new Frame(balanceFrame([], [])).balance().currencies, {});
  assert.equal(new Frame(balanceFrame(coins)).balance().currencies, undefined);
  assert.equal(new Frame(balanceFrame(undefined, currencies)).balance().coins, undefined);
  assert.deepEqual(new Frame(balanceFrame([], [])).balance().coins, {});
});

test('optional currency balances preserve full values, coexist with coins, and clear explicitly', () => {
  const currencies = {
    USDT: { used: 1.25, free: 8.75, total: 10, upnl: -3 },
    USDC: { used: 2, free: 4, total: 6, upnl: 0.5 },
  };
  assert.equal(new Frame(balanceFrame()).balance().currencies, undefined);
  assert.equal(new Frame(balanceFrame([])).balance().currencies, undefined);
  assert.deepEqual(new Frame(balanceFrame(undefined, {})).balance(), { used: 1, free: 2, total: 3, upnl: 4, currencies: {} });
  const currencyOnly = new Frame(balanceFrame(undefined, currencies)).balance();
  assert.deepEqual(currencyOnly.currencies, currencies);
  assert.equal(currencyOnly.coins, undefined);
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
  const tail = at + count * 11 * 8;
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
  TICKER_FIELDS.slice(0, 11).forEach((_, column) => {
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

test('book iterators filter interleaved kinds lazily while preserving repeated markets and later validation', () => {
  for (const kind of [SECTION.BOOK, SECTION.BOOK_DELTA]) {
    const other = kind === SECTION.BOOK ? SECTION.BOOK_DELTA : SECTION.BOOK;
    const width = kind === SECTION.BOOK ? 3 : 2;
    const levels = width === 3 ? [50, 2, 2] : [50, 2];
    const invalid = (section) => ({
      ...section,
      write(view, offset) { section.write(view, offset); view.setUint32(offset + 12, 1, true); },
    });
    const frame = numericFrame([
      bookSection(kind, 7, levels),
      invalid(bookSection(other, 20)),
      { kind: SECTION.STATUS, length: 16, write() {} },
      bookSection(kind, 7),
      bookSection(other, 21),
      bookSection(kind, 9, [], levels),
      invalid(bookSection(kind, 10)),
    ]);
    const read = () => kind === SECTION.BOOK ? frame.books() : frame.bookDeltas();
    const iterator = read();
    assert.equal(iterator[Symbol.iterator](), iterator);
    const first = iterator.next();
    assert.equal(first.value.market, 7);
    assert.equal(first.value.bids.buffer, frame.buffer);
    assert.deepEqual([...first.value.bids], levels);
    const empty = iterator.next().value;
    assert.equal(empty.market, 7);
    assert.equal(empty.bids.length, 0);
    assert.equal(empty.asks.length, 0);
    assert.equal(iterator.next().value.market, 9);
    assert.throws(() => iterator.next(), /invalid book section length/);
    const early = read();
    assert.equal(early.next().value.market, 7);
    assert.deepEqual(early.return(), { value: undefined, done: true });
    assert.deepEqual(early.next(), { value: undefined, done: true });
  }
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

const revisedTable = (kind, count = 2) => {
  const data = Math.ceil((4 + count * ({ [SECTION.MARKETS]: 20, [SECTION.TICKERS]: 4, [SECTION.ORDERS]: 20, [SECTION.POSITIONS]: 8, [SECTION.SETTINGS]: 8, [SECTION.FILLS]: 12 }[kind])) / 8) * 8;
  const bytes = { [SECTION.MARKETS]: 84, [SECTION.TICKERS]: 104, [SECTION.ORDERS]: 96, [SECTION.POSITIONS]: 80, [SECTION.SETTINGS]: 28, [SECTION.FILLS]: 20 }[kind];
  return { kind, revision: 1, length: data + count * bytes,
    write(view, offset) {
      view.setUint32(offset, count, true);
      for (let i = 0; i < count; i++) {
        const symbols = { [SECTION.MARKETS]: 80, [SECTION.TICKERS]: 96, [SECTION.ORDERS]: 92, [SECTION.POSITIONS]: 76, [SECTION.SETTINGS]: 24, [SECTION.FILLS]: 16 }[kind];
        view.setUint32(offset + data + count * symbols + i * 4, i, true);
        if (kind === SECTION.TICKERS) {
          view.setFloat64(offset + data + count * 88 + i * 8, 100 + i, true);
          view.setUint32(offset + data + count * 100 + i * 4, i, true);
        }
        if (kind === SECTION.ORDERS) {
          view.setFloat64(offset + data + count * 56 + i * 8, 3 + i, true);
          view.setFloat64(offset + data + count * 64 + i * 8, 4 + i, true);
          view.setFloat64(offset + data + count * 72 + i * 8, 300 + i, true);
          view.setFloat64(offset + data + count * 80 + i * 8, 101 + i, true);
          view.setUint32(offset + data + count * 88 + i * 4, i ? 0xffff : 1 | (3 << 8), true);
        }
        if (kind === SECTION.POSITIONS) view.setUint32(offset + data + count * 72 + i * 4, i, true);
      }
    },
  };
};
const withStrings = (frame, values = ['BTC-USD', 'ETH-USD']) => {
  frame.strings = stringFrame(values).strings;
  return frame;
};

test('writer-required references reject NONE while preserving explicit empty strings', () => {
  const cases = [];
  const add = (table, reader, fields) => {
    for (const [field, offset] of Object.entries(fields)) {
      cases.push({ make: () => withStrings(numericFrame([table]), ['']), offset,
        read: (frame) => frame[reader]()[field](0), label: `${reader}.${field}` });
    }
  };
  add(revisedTable(SECTION.MARKETS, 1), 'markets', { id: 4, symbol: 8, base: 12, quote: 16 });
  add(revisedTable(SECTION.ORDERS, 1), 'orders', { id: 4, symbol: 116 });
  add(revisedTable(SECTION.POSITIONS, 1), 'positions', { id: 88, symbol: 92 });
  add(revisedTable(SECTION.SETTINGS, 1), 'settings', { symbol: 40 });
  add(revisedTable(SECTION.FILLS, 1), 'fills', { orderId: 4, symbol: 32 });
  const metadata = (kind, length) => ({ kind, length,
    write(view, offset) { view.setUint32(offset, 1, true); } });
  add(metadata(SECTION.CANDLES, 64), 'candles', { interval: 8 });
  add(metadata(SECTION.DERIVATIVE_PRODUCTS, 48), 'derivativeProducts', { underlying: 12 });
  add(metadata(SECTION.POSITION_GREEKS, 48), 'positionGreeks', { positionId: 4, units: 8 });
  add(metadata(SECTION.PREDICTION_MARKETS, 48), 'predictionMarkets', {
    conditionId: 8, outcome: 12, question: 16, description: 20, endDate: 24, slug: 28,
  });
  for (const [field, offset] of [['symbol', 104], ['id', 108]]) {
    cases.push({ make: () => withStrings(numericFrame([revisedTable(SECTION.TICKERS, 1)]), ['']),
      offset, read: (frame) => frame.tickers().identity(0)[field], label: `ticker.${field}` });
  }
  cases.push({ make: () => new Frame(balanceFrame([{ base: '', amount: 1, notional: 2 }])),
    offset: 36, read: (frame) => frame.balance().coins[''].base, label: 'coin.base' });
  cases.push({ make: () => new Frame(balanceFrame(undefined, [{ currency: '', used: 1, free: 2, total: 3, upnl: 0 }])),
    offset: 48, read: (frame) => { frame.balance(); return ''; }, label: 'currency.name' });
  for (const { make, offset, read, label } of cases) {
    const frame = make();
    assert.equal(read(frame), '', `${label}: explicit empty value`);
    new DataView(frame.buffer).setUint32(24 + offset, NONE, true);
    assert.throws(() => read(frame), /required string/, `${label}: absent mandatory value`);
  }
});

test('optional NONE references remain absent independently of required identities', () => {
  const marketTable = revisedTable(SECTION.MARKETS, 1);
  marketTable.length = Math.ceil(marketTable.length / 8) * 8;
  const frame = withStrings(numericFrame([marketTable, revisedTable(SECTION.ORDERS, 1)]), ['']);
  const view = new DataView(frame.buffer);
  view.setUint32(frame.first(SECTION.MARKETS).offset + 104, NONE, true);
  const ordersAt = frame.first(SECTION.ORDERS).offset;
  view.setUint32(ordersAt + 8, NONE, true);
  view.setUint32(ordersAt + 12, NONE, true);
  assert.equal(frame.markets().prettySymbol(0), undefined);
  assert.equal(frame.markets().symbol(0), '');
  assert.equal(frame.orders().id(0), '');
  assert.equal(frame.orders().clientOrderId(0), undefined);
  assert.equal(frame.orders().parentId(0), undefined);
  const metadata = withStrings(numericFrame([
    { kind: SECTION.INSTRUMENT_CLASSIFICATIONS, length: 24, write(view, offset) {
      view.setUint32(offset, 1, true);
      for (const at of [8, 12, 16]) view.setUint32(offset + at, NONE, true);
    } },
    { kind: SECTION.DERIVATIVE_PRODUCTS, length: 48, write(view, offset) {
      view.setUint32(offset, 1, true);
      for (const at of [16, 20, 24, 28]) view.setUint32(offset + at, NONE, true);
    } },
  ]), ['']);
  const labels = metadata.instrumentClassifications();
  for (const field of ['symbolType', 'marketRegion', 'underlyingTicker']) assert.equal(labels[field](0), undefined);
  const products = metadata.derivativeProducts();
  assert.equal(products.underlying(0), '');
  for (const field of ['settleAsset', 'premiumAsset', 'style', 'settlement']) assert.equal(products[field](0), undefined);
  const legacy = positionFrame(false);
  assert.equal(legacy.positions().id(0), undefined);
});

test('revision one preserves pretty symbols, independent order fields and orphan symbols', () => {
  const frame = withStrings(numericFrame([SECTION.MARKETS, SECTION.TICKERS, SECTION.ORDERS, SECTION.POSITIONS, SECTION.SETTINGS, SECTION.FILLS].map((kind) => revisedTable(kind))));
  assert.equal(frame.markets().prettySymbol(0), 'BTC-USD');
  assert.deepEqual([...frame.tickers().midPrice], [100, 101]);
  assert.deepEqual(frame.tickers().identity(1), { symbol: 'ETH-USD', id: 'ETH-USD' });
  const orders = frame.orders();
  assert.deepEqual([...orders.remainingNative], [3, 4]);
  assert.deepEqual([...orders.filledInPlacementUnits], [4, 5]);
  assert.deepEqual([...orders.remainingNotional], [300, 301]);
  assert.deepEqual([...orders.conditionalOrderPrice], [101, 102]);
  assert.equal(orders.tpTriggerType(0), 'mark_price');
  assert.equal(orders.slTriggerType(0), 'index_price');
  assert.equal(orders.tpTriggerType(1), undefined);
  assert.equal(orders.slTriggerType(1), undefined);
  for (const table of [orders, frame.positions(), frame.settings(), frame.fills()]) assert.equal(table.symbol(1), 'ETH-USD');
  assert.equal(frame.positions().id(1), 'ETH-USD');
});

test('revised table extensions cannot borrow bytes from a later section', () => {
  const readers = new Map([[SECTION.MARKETS, 'markets'], [SECTION.TICKERS, 'tickers'], [SECTION.ORDERS, 'orders'], [SECTION.POSITIONS, 'positions'], [SECTION.SETTINGS, 'settings'], [SECTION.FILLS, 'fills']]);
  for (const [kind, reader] of readers) {
    const table = revisedTable(kind);
    const original = numericFrame([table]);
    const section = original.sections[0];
    for (const count of [0, 2]) {
      const bad = numericFrame([{ kind, revision: 1, length: count ? table.length - 8 : 0,
        write(view, offset) { if (count) view.setUint32(offset, 2, true); } },
      { kind: SECTION.STATUS, length: 512, write() {} }]);
      assert.throws(() => bad[reader](), /truncated/, `${reader} truncated extension`);
    }
    assert.equal(section.revision, 1);
  }
});

test('format and known section revisions are checked while future section kinds are skipped', () => {
  const buffer = numericFrame([]).buffer.slice(0);
  new DataView(buffer).setUint16(4, 2, true);
  assert.throws(() => new Frame(buffer), /unsupported frame format/);
  for (const [kind, revision] of [[SECTION.MARKETS, 2], [SECTION.STATUS, 1], [SECTION.STRINGS, 1]]) {
    assert.throws(() => numericFrame([{ kind, revision, length: 8, write() {} }]), /unsupported section/);
  }
  assert.equal(numericFrame([{ kind: 4000, revision: 99, length: 0, write() {} }]).sections[0].revision, 99);
});

test('unknown order, position, margin and trigger enums stay absent', () => {
  const order = orderEnums(0xffffffff);
  assert.equal(order.status, undefined);
  assert.equal(order.type, undefined);
  assert.equal(order.timeInForce, undefined);
  assert.equal(order.triggerType, undefined);
  assert.equal(order.rejectReason, undefined);
  const position = positionEnums(0xffffffff);
  assert.equal(position.side, undefined);
  assert.equal(position.marginType, undefined);
  assert.equal(position.positionMode, undefined);
  const settings = new Frame(settingsFrame([10]));
  new Uint32Array(settings.buffer, 32, 1)[0] = 99;
  assert.equal(settings.settings().marginType(0), undefined);
});

test('TickerState owns numeric columns and identities after source transfer', () => {
  const table = revisedTable(SECTION.TICKERS);
  const frame = withStrings(numericFrame([table]));
  const state = new TickerState();
  state.apply(frame);
  structuredClone(frame.buffer, { transfer: [frame.buffer] });
  structuredClone(frame.strings.bytes.buffer, { transfer: [frame.strings.bytes.buffer] });
  assert.deepEqual([...state.market], [0, 0]);
  assert.deepEqual([...state.columns.midPrice], [100, 101]);
  assert.deepEqual(state.identity(1), { symbol: 'ETH-USD', id: 'ETH-USD' });
  const delta = tickerDeltas([1, 2048, 105]);
  delta.revision = 1;
  state.apply(numericFrame([delta]));
  assert.deepEqual([...state.columns.midPrice], [100, 105]);
  state.apply(numericFrame([tickerTable(100)]));
  assert.equal(state.columns.midPrice, undefined);
  assert.equal(state.identity, undefined);
});

test('owned ticker tables copy one numeric buffer and retain lazy identities after transfer', () => {
  const frame = withStrings(numericFrame([revisedTable(SECTION.TICKERS)]), ['é', '']);
  const raw = frame.tickers();
  for (const [field, name] of TICKER_FIELDS.entries()) {
    raw[name].set([field + 0.5, field === 0 ? NaN : field + 1.5]);
  }
  raw.market.set([7, 9]);
  const decode = TextDecoder.prototype.decode;
  let decodes = 0;
  TextDecoder.prototype.decode = function (...args) {
    decodes++;
    return decode.apply(this, args);
  };
  try {
    const owned = frame.tickers(true);
    const state = new TickerState();
    state.apply(frame);
    assert.equal(decodes, 0);
    assert.equal(raw.last.buffer, frame.buffer);
    assert.equal(raw.market.buffer, frame.buffer);
    assert.notEqual(owned.market.buffer, frame.buffer);
    assert.notEqual(owned.last.buffer, frame.buffer);
    assert.equal(owned.last.buffer.byteLength, TICKER_FIELDS.length * 2 * 8);
    for (const name of TICKER_FIELDS) {
      assert.equal(owned[name].buffer, owned.last.buffer);
      assert.equal(state.columns[name].buffer, state.columns.last.buffer);
      assert.deepEqual([...owned[name]], [...raw[name]]);
      assert.deepEqual([...state.columns[name]], [...raw[name]]);
    }
    raw.last[0] = 900;
    raw.market[0] = 99;
    structuredClone(frame.buffer, { transfer: [frame.buffer] });
    structuredClone(frame.strings.bytes.buffer, { transfer: [frame.strings.bytes.buffer] });
    assert.equal(owned.last[0], 2.5);
    assert.equal(state.columns.last[0], 2.5);
    assert.deepEqual([...owned.market], [7, 9]);
    assert.deepEqual([...state.market], [7, 9]);
    assert.equal(decodes, 0);
    assert.deepEqual(owned.identity(0), { symbol: 'é', id: 'é' });
    assert.deepEqual(owned.identity(1), { symbol: '', id: '' });
    assert.equal(decodes, 2);
    assert.deepEqual(owned.identity(1), { symbol: '', id: '' });
    assert.equal(decodes, 2);
    assert.deepEqual(state.identity(0), { symbol: 'é', id: 'é' });
    assert.deepEqual(state.identity(1), { symbol: '', id: '' });
    assert.equal(decodes, 4);
  } finally {
    TextDecoder.prototype.decode = decode;
  }
});

test('owned tickers validate every required identity reference without decoding strings', () => {
  for (const value of [NONE, 2]) {
    for (let reference = 0; reference < 4; reference++) {
      const frame = withStrings(numericFrame([revisedTable(SECTION.TICKERS)]));
      const section = frame.first(SECTION.TICKERS);
      frame.view.setUint32(section.offset + 208 + reference * 4, value, true);
      const expected = value === NONE ? /missing required string/ : /invalid string reference/;
      assert.throws(() => frame.tickers(true), expected);
      assert.throws(() => new TickerState().apply(frame), expected);
      assert.equal(frame.strings.cache.length, 0);
    }
  }
  const missing = numericFrame([revisedTable(SECTION.TICKERS)]);
  assert.throws(() => missing.tickers(true), /missing STRINGS/);
  assert.throws(() => new TickerState().apply(missing), /missing STRINGS/);
  const empty = numericFrame([revisedTable(SECTION.TICKERS, 0)]).tickers(true);
  assert.equal(empty.count, 0);
  assert.equal(empty.last.length, 0);
});

test('legacy ticker tables omit mid-price and delta revisions bound their masks and rows', () => {
  assert.equal(numericFrame([tickerTable(100)]).tickers().midPrice, undefined);
  for (const values of [[0, 2048, 5], [0, 4], [0], [-1, 0], [0, 1.5, 3]]) {
    assert.throws(() => numericFrame([tickerDeltas(values)]).tickerDeltas(), /ticker delta/);
  }
  const state = new TickerState();
  state.apply(numericFrame([tickerTable(100)]));
  const delta = tickerDeltas([1, 2048, 105]);
  delta.revision = 1;
  state.apply(numericFrame([delta]));
  assert.ok(Number.isNaN(state.columns.midPrice[0]));
  assert.equal(state.columns.midPrice[1], 105);
  assert.throws(() => state.apply(numericFrame([tickerDeltas([2, 1, 6])])), /baseline row/);
});

test('legacy tables and metadata cannot borrow required fields from a following section', () => {
  const readers = new Map([[SECTION.MARKETS, 'markets'], [SECTION.TICKERS, 'tickers'], [SECTION.ORDERS, 'orders'], [SECTION.POSITIONS, 'positions'], [SECTION.SETTINGS, 'settings'], [SECTION.FILLS, 'fills'], [SECTION.CANDLES, 'candles'], [SECTION.BALANCE, 'balance'], [SECTION.STATUS, 'status'], [SECTION.PREDICTION_MARKETS, 'predictionMarkets'], [SECTION.PREDICTION_HOLDINGS, 'predictionHoldings'], [SECTION.INSTRUMENT_CLASSIFICATIONS, 'instrumentClassifications'], [SECTION.DERIVATIVE_PRODUCTS, 'derivativeProducts'], [SECTION.POSITION_GREEKS, 'positionGreeks']]);
  for (const [kind, reader] of readers) {
    const frame = numericFrame([{ kind, length: 8, write(view, offset) { view.setUint32(offset, 3, true); } },
      { kind: 4000, length: 1024, write() {} }]);
    assert.throws(() => frame[reader](), /truncated/i, reader);
  }
});

test('a missing STRINGS section rejects concrete identity references without inventing empty values', () => {
  const frame = numericFrame([revisedTable(SECTION.MARKETS)]);
  const markets = frame.markets();
  assert.equal(markets.count, 2);
  assert.equal(markets.precisionPrice.length, 2);
  for (const field of ['id', 'symbol', 'base', 'quote', 'prettySymbol']) {
    assert.throws(() => markets[field](0), /missing STRINGS/, field);
  }
  for (const reference of [0, 100, -1, NaN, undefined]) {
    assert.throws(() => frame.string(reference), /missing STRINGS/);
  }
  assert.equal(frame.string(NONE), undefined);
});

test('numeric-only frames remain readable without STRINGS while present tables validate references', () => {
  const numeric = numericFrame([tickerTable(100), tickerDeltas([1, 4, 500])]);
  const state = new TickerState();
  state.apply(numeric);
  assert.equal(state.columns.last[1], 500);
  assert.equal(numeric.string(NONE), undefined);
  const strings = stringFrame(['native-id']);
  assert.equal(strings.string(0), 'native-id');
  for (const reference of [1, -1, NaN, undefined]) {
    assert.throws(() => strings.string(reference), /invalid string reference/);
  }
});

function fillFrame(rows, financials) {
  const count = rows.length;
  const data = Math.ceil((4 + count * 12) / 8) * 8;
  const length = data + count * 16 + (financials ? count * 16 : 0);
  const frame = numericFrame([
    { kind: SECTION.FILLS, length, write(view, at) {
      view.setUint32(at, count, true);
      for (let i = 0; i < count; i++) {
        view.setUint32(at + 4 + i * 4, 0, true);
        view.setUint32(at + 4 + count * 4 + i * 4, i, true);
        view.setUint32(at + 4 + count * 8 + i * 4, i % 2, true);
        view.setFloat64(at + data + i * 16, rows[i][0], true);
        view.setFloat64(at + data + i * 16 + 8, rows[i][1], true);
        if (financials) {
          view.setFloat64(at + data + count * 16 + i * 8, financials[i][0], true);
          view.setFloat64(at + data + count * 24 + i * 8, financials[i][1], true);
        }
      }
    } },
    { kind: SECTION.STATUS, length: 16, write(view, at) {
      view.setUint32(at, 31, true);
      view.setFloat64(at + 8, 123, true);
    } },
  ]);
  frame.strings = stringFrame(['fill-order']).strings;
  return frame;
}

test('legacy fills retain exact row offsets without consuming the following section', () => {
  for (const rows of [[], [[100, 2]], [[100, 2], [101, 3]]]) {
    const frame = fillFrame(rows);
    const fills = frame.fills();
    assert.equal(fills.count, rows.length);
    assert.deepEqual([...fills.rows], rows.flat());
    assert.deepEqual([...fills.market], rows.map((_, i) => i));
    assert.deepEqual([...fills.side], rows.map((_, i) => i % 2));
    assert.equal(fills.realizedPnl, undefined);
    assert.equal(fills.fee, undefined);
    if (rows.length) assert.equal(fills.orderId(0), 'fill-order');
    assert.equal(frame.status().latency, 123);
  }
});

test('fill financial tails survive transfer with missing, zero and negative values', () => {
  const frame = fillFrame([[100, 2], [101, 3], [102, 4]], [[NaN, NaN], [0, 0], [-3.5, -0.25]]);
  const fills = frame.fills();
  assert.deepEqual([...fills.rows], [100, 2, 101, 3, 102, 4]);
  assert.deepEqual([...fills.realizedPnl], [NaN, 0, -3.5]);
  assert.deepEqual([...fills.fee], [NaN, 0, -0.25]);
  // A reader using only the legacy prefix still sees the unchanged values.
  const section = frame.first(SECTION.FILLS);
  const legacy = new Float64Array(frame.buffer, section.offset + 40, 6);
  assert.deepEqual([...legacy], [...fills.rows]);
  assert.equal(frame.status().loaded, 31);
});

test('fill decoder rejects truncated and ambiguous tails before constructing column views', () => {
  for (const length of [0, 1, 4, 8, 16, 24, 40, 48, 56]) {
    const frame = numericFrame([{ kind: SECTION.FILLS, length, write(view, at) {
      if (length >= 4) view.setUint32(at, 2, true);
    } }]);
    assert.throws(() => frame.fills(), /invalid fill section|truncated/);
  }
  for (const count of [0x10000000, 0x80000000, 0xffffffff]) {
    const frame = numericFrame([{ kind: SECTION.FILLS, length: 8, write(view, at) {
      view.setUint32(at, count, true);
    } }]);
    assert.throws(() => frame.fills(), /invalid fill section|truncated/);
  }
  for (const length of [40, 56, 64]) {
    const frame = numericFrame([{ kind: SECTION.FILLS, length, write(view, at) {
      view.setUint32(at, 1, true);
    } }]);
    assert.throws(() => frame.fills(), /invalid fill section length/);
  }
});

test('revised fills preserve raw orphan symbols before aligned optional financial columns', () => {
  for (const count of [1, 2, 3]) {
    for (const financials of [false, true]) {
      const data = Math.ceil((4 + count * 12) / 8) * 8;
      const baseline = data + count * 20;
      const financialAt = Math.ceil(baseline / 8) * 8;
      const length = financials ? financialAt + count * 16 : financialAt;
      const make = (size = length) => withStrings(numericFrame([
        { kind: SECTION.FILLS, revision: 1, length: size, write(view, at) {
          view.setUint32(at, count, true);
          for (let i = 0; i < count; i++) {
            view.setUint32(at + 4 + i * 4, 0, true);
            view.setUint32(at + 4 + count * 4 + i * 4, NONE, true);
            view.setUint32(at + 4 + count * 8 + i * 4, i % 2, true);
            view.setFloat64(at + data + i * 16, 100 + i, true);
            view.setFloat64(at + data + i * 16 + 8, i + 1, true);
            view.setUint32(at + data + count * 16 + i * 4, 1, true);
            if (financials) {
              view.setFloat64(at + financialAt + i * 8, i ? -i : 0, true);
              view.setFloat64(at + financialAt + count * 8 + i * 8, i ? NaN : 0, true);
            }
          }
        } },
        { kind: SECTION.STATUS, length: 16, write(view, at) { view.setUint32(at, 31, true); } },
      ]), ['native-order', 'DELISTED-PERP']);
      const frame = make();
      const fills = frame.fills();
      for (let i = 0; i < count; i++) {
        assert.equal(fills.orderId(i), 'native-order');
        assert.equal(fills.market[i], NONE);
        assert.equal(fills.symbol(i), 'DELISTED-PERP');
        assert.deepEqual([...fills.rows.slice(i * 2, i * 2 + 2)], [100 + i, i + 1]);
      }
      assert.equal(frame.status().loaded, 31);
      if (financials) {
        assert.deepEqual([...fills.realizedPnl], Array.from({ length: count }, (_, i) => i ? -i : 0));
        assert.deepEqual([...fills.fee], Array.from({ length: count }, (_, i) => i ? NaN : 0));
      } else {
        assert.equal(fills.realizedPnl, undefined);
        assert.equal(fills.fee, undefined);
      }
      assert.throws(() => make(length + 8).fills(), /invalid fill section length/);
      if (financials) assert.throws(() => make(length - 8).fills(), /invalid fill section length/);
    }
  }
});

test('capabilities decode the words engine/src/capabilities.rs pins', () => {
  const capabilities = (flags, enums) =>
    numericFrame([{ kind: SECTION.CAPABILITIES, length: 8, write: (view, at) => {
      view.setUint32(at, flags, true);
      view.setUint32(at + 4, enums, true);
    } }]).capabilities();
  assert.deepEqual(capabilities(1 | (1 << 10) | (1 << 12) | (1 << 14), 2 | (1 << 4) | (2 << 8) | (3 << 12) | (4 << 16)), {
    clientOrderLookup: true,
    fillHistory: false,
    orderHistory: false,
    incomeHistory: false,
    fillsByOrderId: false,
    accountLeverage: false,
    accountMarginType: false,
    cancelWithSymbols: false,
    amendWithSymbol: false,
    orderBookQuery: false,
    options: true,
    accountTier: false,
    closedPositions: true,
    balanceInfo: false,
    marketRefresh: false,
    stpControls: false,
    bybitExecutionControls: false,
    rejectClientSlippage: false,
    finiteCommandNumbers: false,
    finiteHistoryBounds: false,
    rejectOrderHistoryFilters: false,
    publicCatalogueValidation: false,
    isolatedBookSubscriptions: false,
    inverseClosedPositions: false,
    scheduleCancel: false,
    account: 'afterLoad',
    cancelAll: 'eachPlacedSymbol',
    leverageQuery: 'onMetadata',
    maxLeverage: 'unsupported',
    quantityUnits: 'inverseContractSized',
    positionHistory: 'unsupported',
    closedPositionFormat: 'unsupported',
    fillHistoryTime: 'asProvided',
  });
  assert.deepEqual(capabilities(1 << 14, 0), capabilities(0, 0));
  assert.equal(Object.hasOwn(capabilities(1 << 14, 0), 'hostClock'), false);
  assert.equal(Object.hasOwn(capabilities(1 << 14, 0), 'null'), false);
  assert.equal(capabilities(1 << 15, 0).marketRefresh, true);
  assert.equal(capabilities(1 << 25, 0).scheduleCancel, true);
  assert.equal(capabilities(0, 0).scheduleCancel, false);
  assert.equal(capabilities(0, 3 << 8).leverageQuery, 'onReadAndMetadata');
  const bridge = capabilities(0x1ff << 16, (6 << 20) | (3 << 24) | (2 << 28));
  for (const name of ['stpControls', 'bybitExecutionControls', 'rejectClientSlippage', 'finiteCommandNumbers',
    'finiteHistoryBounds', 'rejectOrderHistoryFilters', 'publicCatalogueValidation',
    'isolatedBookSubscriptions', 'inverseClosedPositions']) assert.equal(bridge[name], true, name);
  assert.equal(bridge.positionHistory, 'wooOrders');
  assert.equal(bridge.closedPositionFormat, 'blofin');
  assert.equal(bridge.fillHistoryTime, 'nonnegativeSecondsOrMilliseconds');
  assert.equal(bridge.quantityUnits, 'base');
  const bitunix = capabilities(0, (7 << 20) | (4 << 24));
  assert.equal(bitunix.positionHistory, 'bitunix');
  assert.equal(bitunix.closedPositionFormat, 'bitunix');
  assert.throws(() => capabilities(0, 8 << 20), /unknown capability positionHistory/);
  assert.throws(() => capabilities(0, 5 << 24), /unknown capability closedPositionFormat/);
  assert.throws(() => capabilities(0, 3 << 28), /unknown capability fillHistoryTime/);
  assert.throws(() => capabilities(0, 4 << 8), /unknown capability leverageQuery/);
  assert.throws(() => capabilities(0, 9), /unknown capability account/);
  assert.equal(numericFrame([]).capabilities(), undefined);
});

test('market flags carry quantity units in bits 8..12', () => {
  assert.equal(marketQuantityUnits(1), 'base');
  assert.equal(marketQuantityUnits(1 | (3 << 8)), 'contractCount');
  assert.throws(() => marketQuantityUnits(9 << 8), /unknown market quantity units/);
});

function executionFrame(count) {
  const data = Math.ceil((4 + count * 12) / 8) * 8;
  const ids = data + count * 20;
  const financialAt = Math.ceil((ids + count * 4) / 8) * 8;
  return withStrings(numericFrame([
    { kind: SECTION.FILLS, revision: 2, length: financialAt + count * 16, write(view, at) {
      view.setUint32(at, count, true);
      for (let i = 0; i < count; i++) {
        view.setUint32(at + 4 + i * 4, 0, true);
        view.setUint32(at + 4 + count * 4 + i * 4, NONE, true);
        view.setUint32(at + 4 + count * 8 + i * 4, i % 2, true);
        view.setFloat64(at + data + i * 16, 100 + i, true);
        view.setFloat64(at + data + i * 16 + 8, i + 1, true);
        view.setUint32(at + data + count * 16 + i * 4, 1, true);
        view.setUint32(at + ids + i * 4, i === 1 ? NONE : i ? 3 : 2, true);
        view.setFloat64(at + financialAt + i * 8, i === 1 ? NaN : -i, true);
        view.setFloat64(at + financialAt + count * 8 + i * 8, i === 1 ? NaN : -i / 10, true);
      }
    } },
    { kind: SECTION.STATUS, length: 16, write(view, at) { view.setUint32(at, 31, true); } },
  ]), ['native-order', 'DELISTED-PERP', '9007199254740997', 'execution-2']);
}

test('revision two fills distinguish execution ids from order ids and preserve aligned financial columns', () => {
  for (const count of [0, 1, 2, 3]) {
    const frame = executionFrame(count);
    const rows = frame.fills();
    assert.equal(frame.first(SECTION.FILLS).revision, 2);
    assert.equal(rows.count, count);
    for (let i = 0; i < count; i++) {
      assert.equal(rows.orderId(i), 'native-order');
      assert.equal(rows.fillId(i), i === 1 ? undefined : i ? 'execution-2' : '9007199254740997');
      assert.equal(rows.symbol(i), 'DELISTED-PERP');
      assert.equal(rows.market[i], NONE);
      assert.deepEqual([...rows.rows.slice(i * 2, i * 2 + 2)], [100 + i, i + 1]);
      assert.equal(rows.realizedPnl[i], i === 1 ? NaN : -i);
      assert.equal(rows.fee[i], i === 1 ? NaN : -i / 10);
    }
    assert.equal(frame.status().loaded, 31);
  }
});

test('revision two rejects invalid identity refs, truncated columns and unsupported future revisions', () => {
  const frame = executionFrame(1);
  const section = frame.first(SECTION.FILLS);
  new DataView(frame.buffer).setUint32(section.offset + 36, 100, true);
  assert.throws(() => frame.fills().fillId(0), /invalid string reference/);
  new DataView(frame.buffer).setUint32(section.offset + 36, NONE, true);
  assert.equal(frame.fills().fillId(0), undefined);
  for (const count of [1, 2, 3]) {
    const valid = executionFrame(count).first(SECTION.FILLS).length;
    for (const length of [4, 8, valid - 16, valid - 8, valid + 8]) {
      const malformed = numericFrame([{ kind: SECTION.FILLS, revision: 2, length,
        write(view, at) { view.setUint32(at, count, true); } }]);
      assert.throws(() => malformed.fills(), /invalid fill section length|truncated/);
    }
  }
  for (const [kind, revision] of [[SECTION.FILLS, 3], [SECTION.SETTINGS, 2]]) {
    assert.throws(() => numericFrame([{ kind, revision, length: 0, write() {} }]), /unsupported section/);
  }
});
