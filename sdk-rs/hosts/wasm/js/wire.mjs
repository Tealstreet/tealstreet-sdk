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
  constructor(buffer, offset) {
    const count = new Uint32Array(buffer, offset, 1)[0];
    this.offsets = new Uint32Array(buffer, offset + 4, count + 1);
    this.bytes = new Uint8Array(buffer, offset + 4 + 4 * (count + 1));
    this.cache = undefined;
  }

  get(index) {
    if (index === NONE) return undefined;
    const cache = this.cache ??= new Array(this.offsets.length - 1);
    let value = cache[index];
    if (value === undefined) {
      value = utf8.decode(this.bytes.subarray(this.offsets[index], this.offsets[index + 1]));
      cache[index] = value;
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
    const header = new DataView(buffer);
    if (header.getUint32(0, true) !== 0x46584353) throw new Error('not a safe-cex frame');
    if (header.getUint16(4, true) !== 1) throw new Error(`unsupported frame format ${header.getUint16(4, true)}`);
    this.buffer = buffer;
    this.time = header.getFloat64(8, true);
    this.sections = [];
    let offset = 16;
    for (let i = header.getUint16(6, true); i > 0; i--) {
      const kind = header.getUint16(offset, true);
      const length = header.getUint32(offset + 4, true);
      const payload = offset + 8;
      if (kind === SECTION.STRINGS) this.strings = new Strings(buffer, payload);
      else this.sections.push({ kind, offset: payload, length });
      offset = payload + length;
    }
  }

  *of(kind) {
    for (const section of this.sections) if (section.kind === kind) yield section;
  }

  first(kind) {
    return this.of(kind).next().value;
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

  /** Tickers, replacing any earlier table: `market[i]` and one column per field. */
  tickers() {
    const section = this.first(SECTION.TICKERS);
    if (!section) return undefined;
    const { buffer } = this;
    const count = new Uint32Array(buffer, section.offset, 1)[0];
    const market = new Uint32Array(buffer, section.offset + 4, count);
    return { count, market, ...columns(buffer, align8(section.offset + 4 + count * 4), count, TICKER_FIELDS) };
  }

  /** Packed `[ticker, mask, ...values]` deltas against the current ticker table. */
  tickerDeltas() {
    const section = this.first(SECTION.TICKER_DELTAS);
    return section && new Float64Array(this.buffer, section.offset, section.length / 8);
  }

  /** Full books: `bids`/`asks` are rows of `[price, amount, total]`. */
  *books() {
    for (const section of this.of(SECTION.BOOK)) {
      const [market, bids, asks] = new Uint32Array(this.buffer, section.offset, 3);
      const rows = section.offset + 16;
      yield {
        market,
        bids: new Float64Array(this.buffer, rows, bids * 3),
        asks: new Float64Array(this.buffer, rows + bids * 24, asks * 3),
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
    const frame = this;
    return {
      count,
      market,
      enums,
      ...columns(buffer, align8(section.offset + 4 + count * 20), count, ORDER_COLUMNS),
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
    return { count, market, enums, ...columns(buffer, align8(section.offset + 4 + count * 8), count, POSITION_COLUMNS) };
  }

  balance() {
    const section = this.first(SECTION.BALANCE);
    if (!section) return undefined;
    const [used, free, total, upnl] = new Float64Array(this.buffer, section.offset, 4);
    return { used, free, total, upnl };
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
  }

  apply(frame) {
    const table = frame.tickers();
    if (table) {
      this.count = table.count;
      this.market = table.market;
      for (const field of TICKER_FIELDS) this.columns[field] = table[field];
    }
    const deltas = frame.tickerDeltas();
    if (!deltas) return;
    const fields = TICKER_FIELDS.map((field) => this.columns[field]);
    for (let i = 0; i < deltas.length; ) {
      const row = deltas[i];
      const mask = deltas[i + 1];
      i += 2;
      for (let field = 0; field < fields.length; field++) {
        if (mask & (1 << field)) fields[field][row] = deltas[i++];
      }
    }
  }
}
