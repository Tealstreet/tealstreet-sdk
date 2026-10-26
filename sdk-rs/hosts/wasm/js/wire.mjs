// Reader for the engine's flush frame (docs/plans/RUST_REWRITE.md,
// "Data shapes: the flush frame"; encoder: engine/src/wire.rs).
// Readers return views over the transferred buffer unless ownership is requested.
// Numbers are typed-array columns; strings are decoded only when asked for.

export const SECTION = {
  STRINGS: 1,
  MARKETS: 2,
  TICKERS: 3,
  TICKER_DELTAS: 4,
  BOOK: 5,
  CANDLES: 6,
  ORDERS: 7,
  POSITIONS: 8,
  BALANCE: 9,
  STATUS: 10,
  FILLS: 11,
  SETTINGS: 12,
  PREDICTION_MARKETS: 13,
  PREDICTION_HOLDINGS: 14,
  BOOK_DELTA: 15,
  INSTRUMENT_CLASSIFICATIONS: 16,
  DERIVATIVE_PRODUCTS: 17,
  POSITION_GREEKS: 18,
  CAPABILITIES: 19,
  UNDERLYING_TOKENS: 20,
  ACCOUNT_READ: 21,
};

export const NONE = 0xffffffff;

export const TICKER_FIELDS = [
  'bid', 'ask', 'last', 'mark', 'index', 'percentage',
  'openInterest', 'fundingRate', 'fundingTime', 'volume', 'quoteVolume', 'midPrice',
];

const LEGACY_TICKER_FIELDS = TICKER_FIELDS.slice(0, 11);
const REVISED_SECTIONS = new Set([SECTION.MARKETS, SECTION.TICKERS, SECTION.TICKER_DELTAS, SECTION.ORDERS, SECTION.POSITIONS, SECTION.FILLS, SECTION.SETTINGS]);
const KNOWN_SECTIONS = new Set(Object.values(SECTION));

export const MARKET_COLUMNS = [
  'precisionAmount', 'precisionPrice', 'minAmount', 'maxAmount', 'minLeverage',
  'maxLeverage', 'minPrice', 'contractSize', 'takerFee', 'makerFee',
];
export const MARKET_FLAG = { ACTIVE: 1, INVERSE: 2, SPOT: 4, PERPETUAL: 8, EXPIRING: 16, READ_ONLY: 32 };
// Bits 8..12 of a market's flags: how its order and position quantities are expressed.
export const QUANTITY_UNITS = ['base', 'nativePlacement', 'inverseNative', 'contractCount', 'inverseContractSized'];
export const marketQuantityUnits = (flags) => {
  const units = QUANTITY_UNITS[(flags >>> 8) & 0xf];
  if (!units) throw new Error(`unknown market quantity units ${(flags >>> 8) & 0xf}`);
  return units;
};
// CAPABILITIES: a flag word (bit order below), then four-bit enum fields.
// Null reserves the retired host-clock bit 14; later flags retain their positions.
const CAPABILITY_FLAGS = [
  'clientOrderLookup', 'fillHistory', 'orderHistory', 'incomeHistory', 'fillsByOrderId', 'accountLeverage',
  'accountMarginType', 'cancelWithSymbols', 'amendWithSymbol', 'orderBookQuery', 'options', 'accountTier',
  'closedPositions', 'balanceInfo', null, 'marketRefresh',
  'stpControls', 'bybitExecutionControls', 'rejectClientSlippage', 'finiteCommandNumbers',
  'finiteHistoryBounds', 'rejectOrderHistoryFilters', 'publicCatalogueValidation',
  'isolatedBookSubscriptions', 'inverseClosedPositions', 'scheduleCancel',
];
const CAPABILITY_ENUMS = [
  ['account', ['unsupported', 'query', 'afterLoad']],
  ['cancelAll', ['eachOpenSymbol', 'eachPlacedSymbol', 'native']],
  ['leverageQuery', ['settings', 'onRead', 'onMetadata', 'onReadAndMetadata']],
  ['maxLeverage', ['market', 'query', 'rails', 'unsupported']],
  ['quantityUnits', QUANTITY_UNITS],
  ['positionHistory', ['unsupported', 'fillsAccount', 'fillsSymbol', 'bybit', 'okx', 'blofin', 'wooOrders', 'bitunix']],
  ['closedPositionFormat', ['unsupported', 'bybit', 'okx', 'blofin', 'bitunix']],
  ['fillHistoryTime', ['asProvided', 'secondsOrMilliseconds', 'nonnegativeSecondsOrMilliseconds']],
];
export const LOADED_FLAG = { BALANCE: 1, ORDERS: 2, MARKETS: 4, TICKERS: 8, POSITIONS: 16 };
export const STATUS_FLAG = { HEDGED: 1 };

export const ORDER_COLUMNS = ['price', 'amount', 'filled', 'remaining', 'lastUpdate', 'takeProfit', 'stopLoss'];
export const ORDER_STATUS = ['open', 'closed', 'canceled', 'creating', 'updating', 'canceling'];
export const ORDER_TYPE = ['market', 'limit', 'conditional', 'stop_market', 'take_profit_market', 'trailing_stop_market'];
export const ORDER_SIDE = ['buy', 'sell'];
export const TIME_IN_FORCE = [undefined, 'GoodTillCancel', 'ImmediateOrCancel', 'FillOrKill', 'PostOnly'];
export const TRIGGER_TYPE = [undefined, 'mark_price', 'last_price', 'index_price'];
export const REJECT_REASON = [undefined, 'PostOnly', 'InsufficientBalance', 'ReduceOnly'];

export const POSITION_COLUMNS = [
  'entryPrice', 'notional', 'leverage', 'unrealizedPnl', 'realizedPnl',
  'contracts', 'liquidationPrice', 'marginSize', 'lastUpdate',
];
export const POSITION_SIDE = ['long', 'short'];
export const MARGIN_TYPE = [undefined, 'isolated', 'cross'];
export const POSITION_MODE = [undefined, 'hedged', 'oneway'];

/** Decode one order's packed enum word. */
export const orderEnums = (word) => {
  const flags = (word >>> 16) & 0xff;
  const last = word >>> 24;
  return {
    status: ORDER_STATUS[word & 0xff],
    type: ORDER_TYPE[(word >>> 8) & 0xff],
    side: ORDER_SIDE[flags & 1],
    reduceOnly: (flags & 2) !== 0,
    close: flags & 8 ? (flags & 4) !== 0 : undefined,
    rejectReason: REJECT_REASON[flags >>> 4],
    timeInForce: TIME_IN_FORCE[last & 0xf],
    triggerType: TRIGGER_TYPE[last >>> 4],
  };
};

/** Decode one position's packed enum word. */
export const positionEnums = (word) => ({
  side: POSITION_SIDE[word & 0xff],
  marginType: MARGIN_TYPE[(word >>> 8) & 0xff],
  positionMode: POSITION_MODE[(word >>> 16) & 0xff],
});

// Native registries may import before their host supplies text codecs.
let utf8 = typeof TextDecoder === 'function' ? new TextDecoder() : undefined;
const align8 = (n) => Math.ceil(n / 8) * 8;
const requireEnd = (section, end) => {
  if (!Number.isSafeInteger(end) || end > section.offset + section.length) throw new Error(`truncated section ${section.kind}`);
};
const rowCount = (frame, section) => {
  requireEnd(section, section.offset + 4);
  return frame.view.getUint32(section.offset, true);
};
const requiredString = (frame, index) => {
  const value = frame.string(index);
  if (value === undefined) throw new Error('missing required string reference');
  return value;
};

class Strings {
  constructor(buffer, offset, length) {
    if (length < 8) throw new Error('invalid STRINGS section');
    const count = new Uint32Array(buffer, offset, 1)[0];
    const headerLength = 4 + 4 * (count + 1);
    if (headerLength > length) throw new Error('invalid STRINGS count');
    this.offsets = new Uint32Array(buffer, offset + 4, count + 1);
    this.bytes = new Uint8Array(buffer, offset + headerLength, length - headerLength);
    let previous = 0;
    for (const end of this.offsets) {
      if (end < previous || end > this.bytes.length) throw new Error('invalid STRINGS offset');
      previous = end;
    }
    if (this.offsets[0] !== 0) throw new Error('invalid STRINGS first offset');
    // Numeric-only readers never need slots for the frame's string table.
    this.cache = [];
  }

  get(index) {
    if (index === NONE) return undefined;
    this.validate(index);
    let value = this.cache[index];
    if (value === undefined) {
      value = (utf8 ??= new TextDecoder()).decode(this.bytes.subarray(this.offsets[index], this.offsets[index + 1]));
      this.cache[index] = value;
    }
    return value;
  }

  validate(index) {
    if (!Number.isInteger(index) || index < 0 || index + 1 >= this.offsets.length) throw new Error('invalid string reference');
  }

  copy() {
    const buffer = this.bytes.buffer.slice(this.offsets.byteOffset - 4, this.bytes.byteOffset + this.bytes.byteLength);
    return new Strings(buffer, 0, buffer.byteLength);
  }
}

const tickerIdentity = (strings, refs, count) => (i) => ({
  symbol: strings.get(refs[i]), id: strings.get(refs[count + i]),
});

const ownedTickerIdentity = (strings, sourceRefs, count) => {
  const refs = sourceRefs.slice();
  for (const index of refs) {
    if (index === NONE) throw new Error('missing required string reference');
    if (!strings) throw new Error('missing STRINGS section for string reference');
    strings.validate(index);
  }
  return tickerIdentity(strings?.copy(), refs, count);
};

/** Float64 columns that follow `u32` data starting at `offset`. */
const columns = (buffer, offset, rows, names) => {
  const out = {};
  for (let i = 0; i < names.length; i++) {
    out[names[i]] = new Float64Array(buffer, offset + i * rows * 8, rows);
  }
  return out;
};

export class Frame {
  constructor(buffer) {
    if (buffer.byteLength < 16) throw new Error('truncated safe-cex frame');
    const header = new DataView(buffer);
    if (header.getUint32(0, true) !== 0x46584353) throw new Error('not a safe-cex frame');
    if (header.getUint16(4, true) !== 1) throw new Error(`unsupported frame format ${header.getUint16(4, true)}`);
    this.buffer = buffer;
    this.view = header;
    this.time = header.getFloat64(8, true);
    this.sections = [];
    let offset = 16;
    for (let i = header.getUint16(6, true); i > 0; i--) {
      if (offset + 8 > buffer.byteLength) throw new Error('truncated frame section header');
      const kind = header.getUint16(offset, true);
      const revision = header.getUint16(offset + 2, true);
      if (KNOWN_SECTIONS.has(kind) && revision > (kind === SECTION.FILLS ? 2 : REVISED_SECTIONS.has(kind) ? 1 : 0)) {
        throw new Error(`unsupported section ${kind} revision ${revision}`);
      }
      const length = header.getUint32(offset + 4, true);
      const payload = offset + 8;
      if (payload + length > buffer.byteLength) throw new Error('truncated frame section payload');
      if (kind === SECTION.STRINGS) {
        if (this.strings) throw new Error('duplicate STRINGS section');
        this.strings = new Strings(buffer, payload, length);
      }
      else this.sections.push({ kind, revision, offset: payload, length });
      offset = payload + length;
    }
    if (offset !== buffer.byteLength) throw new Error('unexpected trailing frame bytes');
  }

  *of(kind) {
    for (const section of this.sections) if (section.kind === kind) yield section;
  }

  first(kind) {
    for (const section of this.sections) if (section.kind === kind) return section;
  }

  string(index) {
    if (index === NONE) return undefined;
    if (!this.strings) throw new Error('missing STRINGS section for string reference');
    return this.strings.get(index);
  }

  /** A synchronous read does not represent a fresh streaming observation. */
  accountRead() {
    const section = this.first(SECTION.ACCOUNT_READ);
    if (section && section.length !== 0) throw new Error('Invalid account read marker');
    return section !== undefined;
  }

  /** Markets, replacing any earlier table: row i is market index i. */
  markets() {
    const section = this.first(SECTION.MARKETS);
    if (!section) return undefined;
    const { buffer } = this;
    const count = rowCount(this, section);
    requireEnd(section, align8(section.offset + 4 + count * 20) + count * MARKET_COLUMNS.length * 8);
    const refs = new Uint32Array(buffer, section.offset + 4, count * 4);
    const flags = new Uint32Array(buffer, section.offset + 4 + count * 16, count);
    const data = align8(section.offset + 4 + count * 20);
    const tail = data + count * MARKET_COLUMNS.length * 8;
    requireEnd(section, tail + (section.revision === 1 ? count * 4 : 0));
    const pretty = section.revision === 1 ? new Uint32Array(buffer, tail, count) : undefined;
    const frame = this;
    return {
      count,
      flags,
      ...(pretty && { prettySymbol: (i) => frame.string(pretty[i]) }),
      ...columns(buffer, data, count, MARKET_COLUMNS),
      id: (i) => requiredString(frame, refs[i * 4]),
      symbol: (i) => requiredString(frame, refs[i * 4 + 1]),
      base: (i) => requiredString(frame, refs[i * 4 + 2]),
      quote: (i) => requiredString(frame, refs[i * 4 + 3]),
    };
  }

  /** Native labels preserve absent and explicitly empty values independently. */
  instrumentClassifications() {
    const section = this.first(SECTION.INSTRUMENT_CLASSIFICATIONS);
    if (!section) return undefined;
    const count = rowCount(this, section);
    if (4 + count * 16 > section.length) throw new Error('Truncated instrument classifications');
    const market = new Uint32Array(this.buffer, section.offset + 4, count);
    const refs = new Uint32Array(this.buffer, section.offset + 4 + count * 4, count * 3);
    return { count, market,
      symbolType: (i) => this.string(refs[i * 3]),
      marketRegion: (i) => this.string(refs[i * 3 + 1]),
      underlyingTicker: (i) => this.string(refs[i * 3 + 2]),
    };
  }

  /** The venue token behind a market (Hyperliquid UBTC behind BTC/USDC). */
  underlyingTokens() {
    const section = this.first(SECTION.UNDERLYING_TOKENS);
    if (!section) return undefined;
    const count = rowCount(this, section);
    if (4 + count * 12 > section.length) throw new Error('Truncated underlying tokens');
    const market = new Uint32Array(this.buffer, section.offset + 4, count);
    const refs = new Uint32Array(this.buffer, section.offset + 4 + count * 4, count * 2);
    return { count, market,
      name: (i) => requiredString(this, refs[i * 2]),
      fullName: (i) => this.string(refs[i * 2 + 1]),
    };
  }

  derivativeProducts() {
    const section = this.first(SECTION.DERIVATIVE_PRODUCTS);
    if (!section) return undefined;
    const count = rowCount(this, section);
    const data = align8(section.offset + 4 + count * 28);
    if (data + count * 16 > section.offset + section.length) throw new Error('Truncated derivative products');
    const market = new Uint32Array(this.buffer, section.offset + 4, count);
    const kind = new Uint32Array(this.buffer, section.offset + 4 + count * 4, count);
    const refs = new Uint32Array(this.buffer, section.offset + 4 + count * 8, count * 5);
    return { count, market, kind,
      expiry: new Float64Array(this.buffer, data, count),
      strike: new Float64Array(this.buffer, data + count * 8, count),
      underlying: (i) => requiredString(this, refs[i * 5]),
      settleAsset: (i) => this.string(refs[i * 5 + 1]),
      premiumAsset: (i) => this.string(refs[i * 5 + 2]),
      style: (i) => this.string(refs[i * 5 + 3]),
      settlement: (i) => this.string(refs[i * 5 + 4]),
    };
  }

  positionGreeks() {
    const section = this.first(SECTION.POSITION_GREEKS);
    if (!section) return undefined;
    const count = rowCount(this, section);
    const data = align8(section.offset + 4 + count * 8);
    if (data + count * 32 > section.offset + section.length) throw new Error('Truncated position Greeks');
    const refs = new Uint32Array(this.buffer, section.offset + 4, count * 2);
    return { count, ...columns(this.buffer, data, count, ['delta', 'gamma', 'vega', 'theta']),
      positionId: (i) => requiredString(this, refs[i * 2]),
      units: (i) => requiredString(this, refs[i * 2 + 1]),
    };
  }

  /** Optional outcome identity/signing metadata, keyed by current market index. */
  predictionMarkets() {
    const section = this.first(SECTION.PREDICTION_MARKETS);
    if (!section) return undefined;
    const { buffer } = this;
    const count = rowCount(this, section);
    requireEnd(section, align8(section.offset + 4 + count * 32) + count * 8);
    const market = new Uint32Array(buffer, section.offset + 4, count);
    const refs = new Uint32Array(buffer, section.offset + 4 + count * 4, count * 6);
    const flags = new Uint32Array(buffer, section.offset + 4 + count * 28, count);
    const tickSize = new Float64Array(buffer, align8(section.offset + 4 + count * 32), count);
    return { count, market, flags, tickSize,
      conditionId: (i) => requiredString(this, refs[i * 6]),
      outcome: (i) => requiredString(this, refs[i * 6 + 1]),
      question: (i) => requiredString(this, refs[i * 6 + 2]),
      description: (i) => requiredString(this, refs[i * 6 + 3]),
      endDate: (i) => requiredString(this, refs[i * 6 + 4]),
      slug: (i) => requiredString(this, refs[i * 6 + 5]),
    };
  }

  /** Optional holding settlement flags; an empty table clears previous flags. */
  predictionHoldings() {
    const section = this.first(SECTION.PREDICTION_HOLDINGS);
    if (!section) return undefined;
    const count = rowCount(this, section);
    requireEnd(section, section.offset + 4 + count * 8);
    return { count,
      market: new Uint32Array(this.buffer, section.offset + 4, count),
      flags: new Uint32Array(this.buffer, section.offset + 4 + count * 4, count),
    };
  }

  /** Tickers replace the earlier table; owned copies survive source transfer. */
  tickers(owned = false) {
    const section = this.first(SECTION.TICKERS);
    if (!section) return undefined;
    const { buffer } = this;
    const count = rowCount(this, section);
    const at = align8(section.offset + 4 + count * 4);
    const fields = section.revision === 1 ? TICKER_FIELDS : LEGACY_TICKER_FIELDS;
    const tail = at + count * fields.length * 8;
    requireEnd(section, tail + (section.revision === 1 ? count * 8 : 0));
    const market = new Uint32Array(buffer, section.offset + 4, count);
    const table = { count, market: owned ? market.slice() : market,
      ...columns(owned ? buffer.slice(at, tail) : buffer, owned ? 0 : at, count, fields) };
    if (section.revision === 1 || (count > 0 && tail + count * 8 <= section.offset + section.length)) {
      const refs = new Uint32Array(buffer, tail, count * 2);
      table.identity = owned ? ownedTickerIdentity(this.strings, refs, count)
        : (i) => ({ symbol: requiredString(this, refs[i]), id: requiredString(this, refs[count + i]) });
    }
    return table;
  }

  /** Packed `[ticker, mask, ...values]` deltas against the current ticker table. */
  tickerDeltas() {
    const section = this.first(SECTION.TICKER_DELTAS);
    if (!section) return undefined;
    if (section.length % 8) throw new Error('invalid ticker delta length');
    const values = new Float64Array(this.buffer, section.offset, section.length / 8);
    const allowed = (1 << (section.revision === 1 ? TICKER_FIELDS.length : LEGACY_TICKER_FIELDS.length)) - 1;
    for (let i = 0; i < values.length;) {
      const row = values[i++], mask = values[i++];
      if (!Number.isInteger(row) || row < 0 || !Number.isInteger(mask) || mask < 0 || mask > allowed) throw new Error('invalid ticker delta header');
      let width = 0;
      for (let bits = mask; bits; bits &= bits - 1) width++;
      i += width;
      if (i > values.length) throw new Error('truncated ticker delta');
    }
    return values;
  }

  /** Full books: `bids`/`asks` are rows of `[price, amount, total]`. */
  books() {
    return this.bookSections(SECTION.BOOK, 3);
  }

  /** Changed `[price, amount]` levels; zero amount deletes a price. */
  bookDeltas() {
    return this.bookSections(SECTION.BOOK_DELTA, 2);
  }

  *bookSections(kind, width) {
    for (const section of this.sections) {
      if (section.kind !== kind) continue;
      if (section.offset % 8 || section.length < 16) throw new Error('invalid book section header');
      const market = this.view.getUint32(section.offset, true);
      const bids = this.view.getUint32(section.offset + 4, true);
      const asks = this.view.getUint32(section.offset + 8, true);
      if (this.view.getUint32(section.offset + 12, true) !== 0
        || section.length !== 16 + (bids + asks) * width * 8) throw new Error('invalid book section length');
      const rows = section.offset + 16;
      yield {
        market,
        bids: new Float64Array(this.buffer, rows, bids * width),
        asks: new Float64Array(this.buffer, rows + bids * width * 8, asks * width),
      };
    }
  }

  candles() {
    const section = this.first(SECTION.CANDLES);
    if (!section) return undefined;
    const { buffer } = this;
    const count = rowCount(this, section);
    requireEnd(section, align8(section.offset + 4 + count * 8) + count * 48);
    const market = new Uint32Array(buffer, section.offset + 4, count);
    const interval = new Uint32Array(buffer, section.offset + 4 + count * 4, count);
    const rows = new Float64Array(buffer, align8(section.offset + 4 + count * 8), count * 6);
    return { count, market, interval: (i) => requiredString(this, interval[i]), rows };
  }

  orders() {
    const section = this.first(SECTION.ORDERS);
    if (!section) return undefined;
    const { buffer } = this;
    const count = rowCount(this, section);
    requireEnd(section, align8(section.offset + 4 + count * 20) + count * ORDER_COLUMNS.length * 8);
    const refs = new Uint32Array(buffer, section.offset + 4, count * 3);
    const market = new Uint32Array(buffer, section.offset + 4 + count * 12, count);
    const enums = new Uint32Array(buffer, section.offset + 4 + count * 16, count);
    const data = align8(section.offset + 4 + count * 20);
    const native = data + count * ORDER_COLUMNS.length * 8;
    const revised = section.revision === 1;
    requireEnd(section, native + (revised ? count * 40 : 0));
    const brackets = revised ? new Uint32Array(buffer, native + count * 32, count) : undefined;
    const symbols = revised ? new Uint32Array(buffer, native + count * 36, count) : undefined;
    const frame = this;
    return {
      count,
      market,
      enums,
      ...columns(buffer, data, count, ORDER_COLUMNS),
      ...(revised && {
        remainingNotional: new Float64Array(buffer, native + count * 16, count),
        conditionalOrderPrice: new Float64Array(buffer, native + count * 24, count),
        tpTriggerType: (i) => TRIGGER_TYPE[brackets[i] & 0xff],
        slTriggerType: (i) => TRIGGER_TYPE[(brackets[i] >>> 8) & 0xff],
        symbol: (i) => requiredString(frame, symbols[i]),
      }),
      ...(count && native + count * 8 <= section.offset + section.length
        ? { remainingNative: new Float64Array(buffer, native, count) }
        : {}),
      ...(count && native + count * 16 <= section.offset + section.length
        ? { filledInPlacementUnits: new Float64Array(buffer, native + count * 8, count) }
        : {}),
      id: (i) => requiredString(frame, refs[i * 3]),
      clientOrderId: (i) => frame.string(refs[i * 3 + 1]),
      parentId: (i) => frame.string(refs[i * 3 + 2]),
    };
  }

  positions() {
    const section = this.first(SECTION.POSITIONS);
    if (!section) return undefined;
    const { buffer } = this;
    const count = rowCount(this, section);
    requireEnd(section, align8(section.offset + 4 + count * 8) + count * POSITION_COLUMNS.length * 8 + (section.revision === 1 ? count * 8 : 0));
    const market = new Uint32Array(buffer, section.offset + 4, count);
    const enums = new Uint32Array(buffer, section.offset + 4 + count * 4, count);
    const data = align8(section.offset + 4 + count * 8);
    const idsAt = data + count * 8 * POSITION_COLUMNS.length;
    const ids = idsAt + count * 4 <= section.offset + section.length
      ? new Uint32Array(buffer, idsAt, count) : undefined;
    const symbols = section.revision === 1 ? new Uint32Array(buffer, idsAt + count * 4, count) : undefined;
    return { count, market, enums, ...columns(buffer, data, count, POSITION_COLUMNS),
      ...(symbols && { symbol: (i) => requiredString(this, symbols[i]) }),
      id: (i) => ids ? requiredString(this, ids[i]) : undefined };
  }

  balance() {
    const section = this.first(SECTION.BALANCE);
    if (!section) return undefined;
    requireEnd(section, section.offset + 32);
    const [used, free, total, upnl] = new Float64Array(this.buffer, section.offset, 4);
    if (section.length <= 32) return { used, free, total, upnl };
    requireEnd(section, section.offset + 36);
    const count = new Uint32Array(this.buffer, section.offset + 32, 1)[0];
    requireEnd(section, align8(section.offset + 36 + count * 4) + count * 16);
    const refs = new Uint32Array(this.buffer, section.offset + 36, count);
    const at = align8(section.offset + 36 + count * 4);
    const amounts = new Float64Array(this.buffer, at, count);
    const notionals = new Float64Array(this.buffer, at + count * 8, count);
    const coins = {};
    for (let i = 0; i < count; i++) {
      const base = requiredString(this, refs[i]);
      Object.defineProperty(coins, base, { value: { base, amount: amounts[i], notional: notionals[i] }, enumerable: true, configurable: true });
    }
    const tail = at + count * 16;
    if (tail >= section.offset + section.length) return { used, free, total, upnl, coins };
    requireEnd(section, tail + 8);
    const n = new Uint32Array(this.buffer, tail, 1)[0];
    requireEnd(section, align8(tail + 8 + n * 4) + n * 32);
    const flags = new Uint32Array(this.buffer, tail + 4, 1)[0];
    const names = new Uint32Array(this.buffer, tail + 8, n);
    const data = align8(tail + 8 + n * 4);
    const totals = new Float64Array(this.buffer, data, n);
    const available = new Float64Array(this.buffer, data + n * 8, n);
    const held = new Float64Array(this.buffer, data + n * 16, n);
    const unrealized = new Float64Array(this.buffer, data + n * 24, n);
    const currencies = {};
    for (let i = 0; i < n; i++) {
      Object.defineProperty(currencies, requiredString(this, names[i]), {
        value: { total: totals[i], free: available[i], used: held[i], upnl: unrealized[i] },
        enumerable: true, configurable: true,
      });
    }
    return { used, free, total, upnl, ...(flags & 1 ? {} : { coins }), currencies };
  }

  /** The venue's capability record; snapshot frames only. */
  capabilities() {
    const section = this.first(SECTION.CAPABILITIES);
    if (!section) return undefined;
    requireEnd(section, section.offset + 8);
    const [flags, enums] = new Uint32Array(this.buffer, section.offset, 2);
    const record = {};
    CAPABILITY_FLAGS.forEach((name, bit) => {
      if (name !== null) record[name] = ((flags >>> bit) & 1) === 1;
    });
    CAPABILITY_ENUMS.forEach(([name, values], field) => {
      const value = values[(enums >>> (field * 4)) & 0xf];
      if (value === undefined) throw new Error(`unknown capability ${name}`);
      record[name] = value;
    });
    return record;
  }

  status() {
    const section = this.first(SECTION.STATUS);
    if (!section) return undefined;
    requireEnd(section, section.offset + 16);
    const [loaded, flags] = new Uint32Array(this.buffer, section.offset, 2);
    const latency = new Float64Array(this.buffer, section.offset + 8, 1)[0];
    return { loaded, hedged: (flags & STATUS_FLAG.HEDGED) !== 0, latency };
  }

  /** Per-market account settings, replacing any earlier table. */
  settings() {
    const section = this.first(SECTION.SETTINGS);
    if (!section) return undefined;
    const { buffer } = this;
    const count = rowCount(this, section);
    requireEnd(section, align8(section.offset + 4 + count * 8) + count * (section.revision === 1 ? 28 : 8));
    const market = new Uint32Array(buffer, section.offset + 4, count);
    const margin = new Uint32Array(buffer, section.offset + 4 + count * 4, count);
    const data = align8(section.offset + 4 + count * 8);
    const leverage = new Float64Array(buffer, data, count);
    const end = section.offset + section.length;
    const buyLeverage = data + count * 16 <= end ? new Float64Array(buffer, data + count * 8, count) : leverage;
    const sellLeverage = data + count * 24 <= end ? new Float64Array(buffer, data + count * 16, count) : leverage;
    const symbols = section.revision === 1 ? new Uint32Array(buffer, data + count * 24, count) : undefined;
    return { count, market, leverage, buyLeverage, sellLeverage,
      ...(symbols && { symbol: (i) => requiredString(this, symbols[i]) }),
      marginType: (i) => MARGIN_TYPE[margin[i]] };
  }

  fills() {
    const section = this.first(SECTION.FILLS);
    if (!section) return undefined;
    const { buffer } = this;
    const count = rowCount(this, section);
    const data = align8(section.offset + 4 + count * 12);
    const numericEnd = data + count * 16;
    // Revision 1 has optional financial tails; revision 2 explicitly adds
    // execution refs and always carries the aligned financial columns.
    const baselineEnd = numericEnd + (section.revision >= 1 ? count * 4 : 0);
    requireEnd(section, baselineEnd);
    const end = section.offset + section.length;
    const idsEnd = baselineEnd + (section.revision === 2 ? count * 4 : 0);
    const financialAt = align8(idsEnd);
    const financials = section.revision === 2 || (end !== baselineEnd && end !== financialAt);
    if (financials && end !== financialAt + count * 16) throw new Error('invalid fill section length');
    const orderId = new Uint32Array(buffer, section.offset + 4, count);
    const market = new Uint32Array(buffer, section.offset + 4 + count * 4, count);
    const side = new Uint32Array(buffer, section.offset + 4 + count * 8, count);
    const rows = new Float64Array(buffer, data, count * 2);
    const symbols = section.revision >= 1 ? new Uint32Array(buffer, numericEnd, count) : undefined;
    const ids = section.revision === 2 ? new Uint32Array(buffer, baselineEnd, count) : undefined;
    return { count, market, side, rows,
      ...(symbols && { symbol: (i) => requiredString(this, symbols[i]) }),
      orderId: (i) => requiredString(this, orderId[i]),
      ...(ids && { fillId: (i) => this.string(ids[i]) }),
      ...(financials ? {
        realizedPnl: new Float64Array(buffer, financialAt, count),
        fee: new Float64Array(buffer, financialAt + count * 8, count),
      } : {}),
    };
  }
}

/**
 * Owned ticker columns and identities survive transferring source frames.
 * Read `state.columns.last[i]` for ticker row i.
 */
export class TickerState {
  constructor() {
    this.count = 0;
    this.market = new Uint32Array(0);
    this.columns = {};
    this.identity = undefined;
  }

  apply(frame) {
    const table = frame.tickers(true);
    if (table) {
      this.count = table.count;
      this.market = table.market;
      this.identity = table.identity;
      for (const field of TICKER_FIELDS) {
        if (table[field]) this.columns[field] = table[field];
        else delete this.columns[field];
      }
    }
    const deltas = frame.tickerDeltas();
    if (!deltas) return;
    for (let i = 0; i < deltas.length; ) {
      const row = deltas[i];
      const mask = deltas[i + 1];
      if (row >= this.count) throw new Error('ticker delta has no baseline row');
      i += 2;
      for (let bits = mask; bits; bits &= bits - 1) {
        const name = TICKER_FIELDS[31 - Math.clz32(bits & -bits)];
        this.columns[name] ??= new Float64Array(this.count).fill(NaN);
        this.columns[name][row] = deltas[i++];
      }
    }
  }
}
