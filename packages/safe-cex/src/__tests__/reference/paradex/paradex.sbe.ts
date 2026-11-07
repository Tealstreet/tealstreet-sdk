import { bigIntAsStringJsonParse } from '@tealstreet/json-parse/src/jsonParse';

// Official schema 1:2: tradeparadex/paradex-py/paradex_py/api/sbe/paradex_1_0.xml.
const NULL = -(1n << 63n);
const utf8 = new TextDecoder('utf-8', { fatal: true });
const sides = ['UNSPECIFIED', 'BUY', 'SELL'];
const orderTypes = ['', 'LIMIT', 'MARKET', 'STOP_LIMIT', 'STOP_MARKET', 'TAKE_PROFIT_LIMIT', 'TAKE_PROFIT_MARKET', 'STOP_LOSS_LIMIT', 'STOP_LOSS_MARKET'];
const orderStatuses = ['', 'NEW', 'UNTRIGGERED', 'OPEN', 'CLOSED'];
const instructions = ['', 'GTC', 'IOC', 'POST_ONLY', 'RPI'];
const orderFlags = ['REDUCE_ONLY', 'STOP_CONDITION_BELOW_TRIGGER', 'STOP_CONDITION_ABOVE_TRIGGER', 'INTERACTIVE', 'TARGET_STRATEGY_VWAP', 'MMP'];
const fixed = (value: bigint, decimals = 8): string | undefined => {
  if (value === NULL) return;
  const sign = value < 0n ? '-' : '';
  const magnitude = value < 0n ? -value : value;
  const scale = 10n ** BigInt(decimals);
  return `${sign}${magnitude / scale}.${String(magnitude % scale).padStart(decimals, '0')}`;
};

class FrameReader {
  readonly view: DataView;
  readonly blockLength: number;
  readonly template: number;
  readonly version: number;
  position: number;

  constructor(readonly bytes: Uint8Array) {
    this.view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
    if (bytes.length < 8 || this.view.getUint16(4, true) !== 1) throw new Error('Invalid Paradex SBE schema');
    this.blockLength = this.view.getUint16(0, true);
    this.template = this.view.getUint16(2, true);
    this.version = this.view.getUint16(6, true);
    this.position = 8 + this.blockLength;
    if (this.version < 1 || this.version > 2 || this.position > bytes.length) throw new Error('Unsupported or truncated Paradex SBE frame');
  }
  require(length: number) {
    if (this.blockLength < length) throw new Error('Truncated Paradex SBE block');
  }
  i64(offset: number) {
    this.require(offset + 8);
    return this.view.getBigInt64(8 + offset, true);
  }
  byte(offset: number) {
    this.require(offset + 1);
    return this.view.getUint8(8 + offset);
  }
  number(offset: number, decimals = 8) { return fixed(this.i64(offset), decimals); }
  time(offset: number) {
    const value = this.i64(offset);
    if (value === NULL) return;
    const milliseconds = value / 1000n;
    if (milliseconds > BigInt(Number.MAX_SAFE_INTEGER) || milliseconds < 0n) throw new Error('Invalid Paradex SBE timestamp');
    return Number(milliseconds);
  }
  string(optional = false): string | undefined {
    if (optional && this.position === this.bytes.length) return;
    if (this.position >= this.bytes.length) throw new Error('Missing Paradex SBE string');
    const length = this.bytes[this.position++];
    if (length > this.bytes.length - this.position) throw new Error('Truncated Paradex SBE string');
    const text = utf8.decode(this.bytes.subarray(this.position, this.position + length));
    this.position += length;
    return text;
  }
  levels(side: 'BUY' | 'SELL') {
    if (this.position + 4 > this.bytes.length) throw new Error('Missing Paradex SBE group');
    const block = this.view.getUint16(this.position, true);
    const count = this.view.getUint16(this.position + 2, true);
    this.position += 4;
    if (block < 16 || count * block > this.bytes.length - this.position) throw new Error('Invalid Paradex SBE group');
    const result = [];
    for (let i = 0; i < count; i++) {
      const price = fixed(this.view.getBigInt64(this.position, true));
      const size = fixed(this.view.getBigInt64(this.position + 8, true));
      if (price === undefined || size === undefined) throw new Error('Null Paradex SBE book level');
      result.push({ side, price, size });
      this.position += block;
    }
    return result;
  }
}

const flags = (bits: number, names: string[]) => names.filter((_name, bit) => bits & (1 << bit));

/** Normalise the official binary wire fields into the existing venue handlers. */
export const decodeParadexMessage = (data: unknown): Record<string, any> | null => {
  if (typeof data === 'string') return bigIntAsStringJsonParse(data);
  const bytes = data instanceof ArrayBuffer ? new Uint8Array(data)
    : ArrayBuffer.isView(data) ? new Uint8Array(data.buffer, data.byteOffset, data.byteLength) : undefined;
  if (!bytes) throw new Error('Unsupported Paradex websocket payload');
  const r = new FrameReader(bytes);
  const requiredNumbers: Record<number, number[]> = {
    1: [25, 33], 4: [16, 24, 32, 40, 48, 56, 80, 88],
    20: [36, 44], 21: [19, 27, 35, 91, 99],
    22: [17, 25, 33, 41, 65, 113, 121, 129, 137, 145],
    23: [16, 24, 32, 40, 48, 104],
  };
  if (requiredNumbers[r.template]) {
    for (const offset of requiredNumbers[r.template]) {
      if (r.i64(offset) === NULL) throw new Error('Null mandatory Paradex SBE field');
    }
  }
  let channel: string;
  let row: Record<string, unknown>;
  const common = () => ({ timestamp: r.time(0), seq_no: String(r.i64(8)) });
  switch (r.template) {
    case 1: {
      r.require(50);
      const market = r.string()!;
      const id = r.version >= 2 ? r.string(true) : undefined;
      channel = `trades.${market}`;
      row = { ...common(), market, id: id || String(r.i64(16)), side: sides[r.byte(24)],
        price: r.number(25), size: r.number(33), created_at: r.time(41) };
      break;
    }
    case 2: {
      r.require(48);
      const market = r.string()!;
      channel = `bbo.${market}`;
      row = { ...common(), market, bid_price: r.number(16), bid_size: r.number(24), ask_price: r.number(32), ask_size: r.number(40) };
      break;
    }
    case 3: {
      r.require(89);
      const kind = r.byte(16);
      if (kind !== 0 && kind !== 1) throw new Error('Invalid Paradex SBE book package');
      const levels = [...r.levels('BUY'), ...r.levels('SELL')];
      const market = r.string()!;
      channel = `order_book.${market}.deltas`;
      row = { ...common(), market, update_type: kind === 0 ? 's' : 'd',
        inserts: kind === 0 ? levels : [], updates: kind === 1 ? levels : [], deletes: [] };
      break;
    }
    case 4: {
      r.require(240);
      const market = r.string()!;
      channel = 'markets_summary';
      row = { ...common(), symbol: market, mark_price: r.number(16), underlying_price: r.number(24),
        last_traded_price: r.number(32), volume_24h: r.number(40), open_interest: r.number(48),
        funding_rate: (r.blockLength >= 240 ? r.number(232, 12) : undefined) ?? r.number(56),
        bid: r.number(64), ask: r.number(72), total_volume: r.number(80), price_change_rate_24h: r.number(88) };
      break;
    }
    case 20: {
      r.require(126);
      if (!orderStatuses[r.byte(16)] || ![1, 2].includes(r.byte(17)) || !orderTypes[r.byte(18)] || !instructions[r.byte(19)]) {
        throw new Error('Unsupported Paradex SBE order enum');
      }
      const id = r.string()!;
      const client_id = r.string()!;
      const market = r.string()!;
      const cancel_reason = r.string()!;
      const requestId = r.version >= 2 ? r.string(true) : undefined;
      const requestMessage = r.version >= 2 ? r.string(true) : undefined;
      channel = 'orders.ALL';
      row = { ...common(), id, client_id, market, cancel_reason,
        status: orderStatuses[r.byte(16)], side: sides[r.byte(17)], type: orderTypes[r.byte(18)],
        instruction: instructions[r.byte(19)], time_in_force: instructions[r.byte(19)],
        price: r.number(20), trigger_price: r.number(28), size: r.number(36), remaining_size: r.number(44),
        avg_fill_price: r.number(52), created_at: r.time(60), last_updated_at: r.time(68),
        flags: flags(r.byte(125), orderFlags),
        ...(r.blockLength >= 128 && (r.byte(126) || r.byte(127)) ? { request_info: {
          id: requestId ?? '', message: requestMessage ?? '',
          status: ['UNSPECIFIED', 'PENDING', 'PROCESSED', 'REJECTED', 'SUCCESS'][r.byte(126)],
          request_type: ['UNSPECIFIED', 'MODIFY_ORDER'][r.byte(127)],
        } } : {}),
      };
      break;
    }
    case 21: {
      r.require(r.version >= 2 ? 116 : 107);
      if (![1, 2].includes(r.byte(17))) throw new Error('Unsupported Paradex SBE fill side');
      const id = r.string()!;
      const order_id = r.string()!;
      const client_id = r.string()!;
      const trade_id = r.string()!;
      const market = r.string()!;
      const fee_currency = r.version >= 2 ? r.string() : undefined;
      channel = 'fills.ALL';
      row = { ...common(), id, order_id, client_id, trade_id, market, fee_currency,
        side: sides[r.byte(17)], liquidity: ['', 'MAKER', 'TAKER'][r.byte(18)],
        price: r.number(19), size: r.number(27), fee: r.number(35), realized_pnl: r.number(43),
        created_at: r.time(51), underlying_price: r.number(91), realized_funding: r.number(99) };
      break;
    }
    case 22: {
      r.require(154);
      if (![1, 2].includes(r.byte(16)) || ![1, 2].includes(r.byte(153))) throw new Error('Unsupported Paradex SBE position enum');
      const market = r.string()!;
      const last_fill_id = r.string()!;
      const size = r.i64(17);
      channel = 'positions';
      row = { ...common(), market, last_fill_id, side: ['', 'LONG', 'SHORT'][r.byte(16)],
        size: fixed(r.byte(16) === 2 && size > 0n ? -size : size), average_entry_price: r.number(25),
        unrealized_pnl: r.number(33), realized_positional_pnl: r.number(41), mark_price: r.number(49),
        liquidation_price: r.number(57), leverage: r.number(65), last_updated_at: r.time(73),
        average_entry_price_usd: r.number(113), cost: r.number(121), cost_usd: r.number(129),
        cached_funding_index: r.number(137), unrealized_funding_pnl: r.number(145),
        status: ['', 'OPEN', 'CLOSED', 'UNSPECIFIED'][r.byte(153)] };
      break;
    }
    case 23: {
      r.require(113);
      const settlement_asset = r.string()!;
      channel = 'balance_events';
      row = { ...common(), settlement_asset, total_collateral: r.number(16), free_collateral: r.number(24),
        initial_margin_requirement: r.number(32), maintenance_margin_requirement: r.number(40),
        account_value: r.number(48), unrealized_pnl: r.number(56), updated_at: r.time(64),
        margin_cushion: r.number(104), status: ['', 'ACTIVE', 'LIQUIDATION'][r.byte(112)],
        ...(r.blockLength >= 121 ? { last_seen_notification: r.time(113) } : {}) };
      break;
    }
    case 40:
      r.require(16);
      if (r.position !== bytes.length) throw new Error('Unexpected Paradex SBE heartbeat data');
      return { type: 'pong' };
    default:
      return null;
  }
  if (r.position !== bytes.length) throw new Error('Unexpected Paradex SBE trailing data');
  return { jsonrpc: '2.0', method: 'subscription', params: { channel, data: row } };
};
