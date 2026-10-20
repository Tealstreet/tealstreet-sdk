// Reader for the engine's flush frame (docs/plans/RUST_REWRITE.md,
// "Data shapes: the flush frame"; encoder: engine/src/wire.rs).
// Everything is a view over the transferred buffer: numbers are typed-array
// columns, strings are decoded only when asked for.

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
};

export const NONE = 0xffffffff;

export const TICKER_FIELDS = [
  'bid', 'ask', 'last', 'mark', 'index', 'percentage',
  'openInterest', 'fundingRate', 'fundingTime', 'volume', 'quoteVolume',
];

export const MARKET_COLUMNS = [
  'precisionAmount', 'precisionPrice', 'minAmount', 'maxAmount', 'minLeverage',
  'maxLeverage', 'minPrice', 'contractSize', 'takerFee', 'makerFee',
];
export const MARKET_FLAG = { ACTIVE: 1, INVERSE: 2, SPOT: 4, PERPETUAL: 8, EXPIRING: 16, READ_ONLY: 32 };
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

const utf8 = new TextDecoder();
const align8 = (n) => (n + 7) & ~7;

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
    if (!Number.isInteger(index) || index < 0 || index + 1 >= this.offsets.length) throw new Error('invalid string reference');
    let value = this.cache[index];
    if (value === undefined) {
      value = utf8.decode(this.bytes.subarray(this.offsets[index], this.offsets[index + 1]));
      this.cache[index] = value;
    }
    return value;
  }
}

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
      const length = header.getUint32(offset + 4, true);
      const payload = offset + 8;
      if (payload + length > buffer.byteLength) throw new Error('truncated frame section payload');
      if (kind === SECTION.STRINGS) {
        if (this.strings) throw new Error('duplicate STRINGS section');
        this.strings = new Strings(buffer, payload, length);
      }
      else this.sections.push({ kind, offset: payload, length });
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
    return this.strings?.get(index);
  }

  /** Markets, replacing any earlier table: row i is market index i. */
  markets() {
    const section = this.first(SECTION.MARKETS);
    if (!section) return undefined;
    const { buffer } = this;
    const count = new Uint32Array(buffer, section.offset, 1)[0];
    const refs = new Uint32Array(buffer, section.offset + 4, count * 4);
    const flags = new Uint32Array(buffer, section.offset + 4 + count * 16, count);
    const data = align8(section.offset + 4 + count * 20);
    const frame = this;
    return {
      count,
      flags,
      ...columns(buffer, data, count, MARKET_COLUMNS),
      id: (i) => frame.string(refs[i * 4]),
      symbol: (i) => frame.string(refs[i * 4 + 1]),
      base: (i) => frame.string(refs[i * 4 + 2]),
      quote: (i) => frame.string(refs[i * 4 + 3]),
    };
  }

  /** Optional outcome identity/signing metadata, keyed by current market index. */
  predictionMarkets() {
    const section = this.first(SECTION.PREDICTION_MARKETS);
    if (!section) return undefined;
    const { buffer } = this;
    const count = this.view.getUint32(section.offset, true);
    const market = new Uint32Array(buffer, section.offset + 4, count);
    const refs = new Uint32Array(buffer, section.offset + 4 + count * 4, count * 6);
    const flags = new Uint32Array(buffer, section.offset + 4 + count * 28, count);
    const tickSize = new Float64Array(buffer, align8(section.offset + 4 + count * 32), count);
    return { count, market, flags, tickSize,
      conditionId: (i) => this.string(refs[i * 6]),
      outcome: (i) => this.string(refs[i * 6 + 1]),
      question: (i) => this.string(refs[i * 6 + 2]),
      description: (i) => this.string(refs[i * 6 + 3]),
      endDate: (i) => this.string(refs[i * 6 + 4]),
      slug: (i) => this.string(refs[i * 6 + 5]),
    };
  }

  /** Optional holding settlement flags; an empty table clears previous flags. */
  predictionHoldings() {
    const section = this.first(SECTION.PREDICTION_HOLDINGS);
    if (!section) return undefined;
    const count = this.view.getUint32(section.offset, true);
    return { count,
      market: new Uint32Array(this.buffer, section.offset + 4, count),
      flags: new Uint32Array(this.buffer, section.offset + 4 + count * 4, count),
    };
  }

  /** Tickers, replacing any earlier table: `market[i]` and one column per field. */
  tickers() {
    const section = this.first(SECTION.TICKERS);
    if (!section) return undefined;
    const { buffer } = this;
    const count = new Uint32Array(buffer, section.offset, 1)[0];
    const market = new Uint32Array(buffer, section.offset + 4, count);
    const at = align8(section.offset + 4 + count * 4);
    const tail = at + count * TICKER_FIELDS.length * 8;
    const table = { count, market, ...columns(buffer, at, count, TICKER_FIELDS) };
    if (count > 0 && tail + count * 8 <= section.offset + section.length) {
      const refs = new Uint32Array(buffer, tail, count * 2);
      table.identity = (i) => ({ symbol: this.string(refs[i]), id: this.string(refs[count + i]) });
    }
    return table;
  }

  /** Packed `[ticker, mask, ...values]` deltas against the current ticker table. */
  tickerDeltas() {
    const section = this.first(SECTION.TICKER_DELTAS);
    return section && new Float64Array(this.buffer, section.offset, section.length / 8);
  }

  /** Full books: `bids`/`asks` are rows of `[price, amount, total]`. */
  *books() {
    yield* this.bookSections(SECTION.BOOK, 3);
  }

  /** Changed `[price, amount]` levels; zero amount deletes a price. */
  *bookDeltas() {
    yield* this.bookSections(SECTION.BOOK_DELTA, 2);
  }

  *bookSections(kind, width) {
    for (const section of this.of(kind)) {
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
    const count = new Uint32Array(buffer, section.offset, 1)[0];
    const market = new Uint32Array(buffer, section.offset + 4, count);
    const interval = new Uint32Array(buffer, section.offset + 4 + count * 4, count);
    const rows = new Float64Array(buffer, align8(section.offset + 4 + count * 8), count * 6);
    return { count, market, interval: (i) => this.string(interval[i]), rows };
  }

  orders() {
    const section = this.first(SECTION.ORDERS);
    if (!section) return undefined;
    const { buffer } = this;
    const count = new Uint32Array(buffer, section.offset, 1)[0];
    const refs = new Uint32Array(buffer, section.offset + 4, count * 3);
    const market = new Uint32Array(buffer, section.offset + 4 + count * 12, count);
    const enums = new Uint32Array(buffer, section.offset + 4 + count * 16, count);
    const data = align8(section.offset + 4 + count * 20);
    const native = data + count * ORDER_COLUMNS.length * 8;
    const frame = this;
    return {
      count,
      market,
      enums,
      ...columns(buffer, data, count, ORDER_COLUMNS),
      ...(count && native + count * 8 <= section.offset + section.length
        ? { remainingNative: new Float64Array(buffer, native, count) }
        : {}),
      id: (i) => frame.string(refs[i * 3]),
      clientOrderId: (i) => frame.string(refs[i * 3 + 1]),
      parentId: (i) => frame.string(refs[i * 3 + 2]),
    };
  }

  positions() {
    const section = this.first(SECTION.POSITIONS);
    if (!section) return undefined;
    const { buffer } = this;
    const count = new Uint32Array(buffer, section.offset, 1)[0];
    const market = new Uint32Array(buffer, section.offset + 4, count);
    const enums = new Uint32Array(buffer, section.offset + 4 + count * 4, count);
    const data = align8(section.offset + 4 + count * 8);
    const idsAt = data + count * 8 * POSITION_COLUMNS.length;
    const ids = idsAt + count * 4 <= section.offset + section.length
      ? new Uint32Array(buffer, idsAt, count) : undefined;
    return { count, market, enums, ...columns(buffer, data, count, POSITION_COLUMNS),
      id: (i) => ids ? this.string(ids[i]) : undefined };
  }

  balance() {
    const section = this.first(SECTION.BALANCE);
    if (!section) return undefined;
    const { buffer } = this;
    const end = section.offset + section.length;
    const requireBytes = (at, bytes) => {
      if (at < section.offset || bytes < 0 || at + bytes > end || end > buffer.byteLength)
        throw new RangeError('Malformed balance section');
    };
    requireBytes(section.offset, 32);
    const [used, free, total, upnl] = new Float64Array(buffer, section.offset, 4);
    if (section.length === 32) return { used, free, total, upnl };
    requireBytes(section.offset + 32, 4);
    const count = this.view.getUint32(section.offset + 32, true);
    requireBytes(section.offset + 36, count * 4);
    const refs = new Uint32Array(buffer, section.offset + 36, count);
    const at = Math.ceil((section.offset + 36 + count * 4) / 8) * 8;
    requireBytes(at, count * 16);
    const amounts = new Float64Array(buffer, at, count);
    const notionals = new Float64Array(buffer, at + count * 8, count);
    const coins = {};
    for (let i = 0; i < count; i++) {
      const base = this.string(refs[i]);
      Object.defineProperty(coins, base, { value: { base, amount: amounts[i], notional: notionals[i] }, enumerable: true, configurable: true });
    }
    const tail = at + count * 16;
    if (tail === end) return { used, free, total, upnl, coins };
    requireBytes(tail, 4);
    const n = this.view.getUint32(tail, true);
    requireBytes(tail + 4, n * 4);
    const names = new Uint32Array(buffer, tail + 4, n);
    const data = Math.ceil((tail + 4 + n * 4) / 8) * 8;
    requireBytes(data, n * 32);
    const currencyEnd = data + n * 32;
    let flags = 0;
    if (currencyEnd !== end) {
      requireBytes(currencyEnd, 8);
      flags = this.view.getUint32(currencyEnd + 4, true);
      if (currencyEnd + 8 !== end || this.view.getUint32(currencyEnd, true) !== 0x314d4342
        || (flags & ~1) || ((flags & 1) && count !== 0))
        throw new RangeError('Malformed balance currency mode footer');
    }
    const held = new Float64Array(buffer, data, n);
    const available = new Float64Array(buffer, data + n * 8, n);
    const totals = new Float64Array(buffer, data + n * 16, n);
    const unrealized = new Float64Array(buffer, data + n * 24, n);
    const currencies = {};
    for (let i = 0; i < n; i++) {
      Object.defineProperty(currencies, this.string(names[i]), {
        value: { used: held[i], free: available[i], total: totals[i], upnl: unrealized[i] },
        enumerable: true, configurable: true,
      });
    }
    return { used, free, total, upnl, ...(flags & 1 ? {} : { coins }), currencies };
  }

  status() {
    const section = this.first(SECTION.STATUS);
    if (!section) return undefined;
    const [loaded, flags] = new Uint32Array(this.buffer, section.offset, 2);
    const latency = new Float64Array(this.buffer, section.offset + 8, 1)[0];
    return { loaded, hedged: (flags & STATUS_FLAG.HEDGED) !== 0, latency };
  }

  /** Per-market account settings, replacing any earlier table. */
  settings() {
    const section = this.first(SECTION.SETTINGS);
    if (!section) return undefined;
    const { buffer } = this;
    const count = new Uint32Array(buffer, section.offset, 1)[0];
    const market = new Uint32Array(buffer, section.offset + 4, count);
    const margin = new Uint32Array(buffer, section.offset + 4 + count * 4, count);
    const data = align8(section.offset + 4 + count * 8);
    const leverage = new Float64Array(buffer, data, count);
    const end = section.offset + section.length;
    const buyLeverage = data + count * 16 <= end ? new Float64Array(buffer, data + count * 8, count) : leverage;
    const sellLeverage = data + count * 24 <= end ? new Float64Array(buffer, data + count * 16, count) : leverage;
    return { count, market, leverage, buyLeverage, sellLeverage, marginType: (i) => MARGIN_TYPE[margin[i]] };
  }

  fills() {
    const section = this.first(SECTION.FILLS);
    if (!section) return undefined;
    const { buffer } = this;
    const count = new Uint32Array(buffer, section.offset, 1)[0];
    const orderId = new Uint32Array(buffer, section.offset + 4, count);
    const market = new Uint32Array(buffer, section.offset + 4 + count * 4, count);
    const side = new Uint32Array(buffer, section.offset + 4 + count * 8, count);
    const rows = new Float64Array(buffer, align8(section.offset + 4 + count * 12), count * 2);
    return { count, market, side, rows, orderId: (i) => this.string(orderId[i]) };
  }
}

/**
 * Live ticker columns for a consumer: adopts each replacement table and
 * applies deltas in place. Read `state.columns.last[i]` for ticker row i.
 */
export class TickerState {
  constructor() {
    this.count = 0;
    this.market = new Uint32Array(0);
    this.columns = {};
    this.identity = undefined;
  }

  apply(frame) {
    const table = frame.tickers();
    if (table) {
      this.count = table.count;
      this.market = table.market;
      this.identity = table.identity;
      for (const field of TICKER_FIELDS) this.columns[field] = table[field];
    }
    const deltas = frame.tickerDeltas();
    if (!deltas) return;
    for (let i = 0; i < deltas.length; ) {
      const row = deltas[i];
      const mask = deltas[i + 1];
      i += 2;
      for (let field = 0; field < TICKER_FIELDS.length; field++) {
        if (mask & (1 << field)) this.columns[TICKER_FIELDS[field]][row] = deltas[i++];
      }
    }
  }
}
