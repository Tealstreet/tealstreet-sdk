// Coinbase INTX moved to the drb gateway (JSON-RPC 2.0, coinbase_cdp auth). The TS
// oracle's INTX implementation is retired (coinbase.starbase.ts is inert), so this
// tape replays Rust only and carries its own expected account state.
// Payloads follow Coinbase's migration guide and adv-starbase-openapi.json; they are
// synthetic, never captured from a live account.
// https://docs.cdp.coinbase.com/coinbase-app/advanced-trade-apis/guides/derivatives/technical
import { COINBASE_REPLAY, COINBASE_ES256_REPLAY, verifyCoinbaseCdpJwt } from './coinbase-protocol.mjs';

export const DRB_HTTP = 'https://drb.coinbase.com/api/v2';
export const DRB_PRIVATE_SOCKET = 'wss://drb.coinbase.com/ws/api/v2';
export const DRB_PUBLIC_SOCKET = 'wss://streams.drb.coinbase.com/ws/api/v2';
export const DRB_RUST_ONLY = 'rust-only';
const TOKEN = 'replay-drb-token';
const SOCKET_TOKEN = 'replay-drb-socket-token';
const INSTRUMENT = 'BTC_USDC-PERPETUAL';
const PRIVATE_CHANNELS = [
  'user.changes.future.USDC.100ms',
  'user.portfolio.USDC',
  'user.isolated.changes.future.USDC.100ms',
];

const isDrb = (url) => {
  try {
    const host = new URL(url).hostname;
    return host === 'drb.coinbase.com' || host === 'streams.drb.coinbase.com';
  } catch {
    return false;
  }
};
export const isDrbUrl = isDrb;
export const isRustOnlyTape = (records) => (records ?? []).some((record) => record.type === 'meta' && record.oracle === DRB_RUST_ONLY);

// Writes mirror S0's closed classifier (drb.rs method_intent: Place, Cancel, Amend,
// Setting); coinbase-drb-tape.test.mjs checks this set against that source.
const DRB_WRITES = new Set([
  'buy', 'sell', 'close_position',
  'cancel', 'cancel_all', 'cancel_all_by_currency', 'cancel_all_by_currency_pair', 'cancel_all_by_instrument',
  'cancel_all_by_kind_or_type', 'cancel_by_label',
  'edit', 'edit_by_label',
  'change_margin_model', 'create_combo', 'disable_cancel_on_disconnect', 'enable_cancel_on_disconnect', 'logout',
  'move_positions', 'set_leverage', 'submit_transfer_between_subaccounts',
].map((method) => `/api/v2/private/${method}`));
export const isDrbWrite = (url) => DRB_WRITES.has(new URL(url).pathname);

/** drb request identity: the JSON-RPC id is per-connection correlation, and the
 * auth JWT is verified separately, so neither is part of the matched request. */
export const drbRequestBody = (parsed, { url }) => {
  if (!isDrb(url) || !parsed || typeof parsed !== 'object') return parsed;
  const { id: _id, ...rest } = parsed;
  if (rest.method === 'public/auth' && rest.params?.grant_type === 'coinbase_cdp')
    return { ...rest, params: { ...rest.params, token: '<verified-jwt>' } };
  return rest;
};

/** HTTP auth signs `POST drb.coinbase.com/api/v2/public/auth`; private methods carry
 * the bearer that auth returned; public reads carry no credential. */
export const verifyDrbRequest = ({ method, url, body, headers, now = Date.now() }) => {
  const path = new URL(url).pathname;
  const entries = Array.isArray(headers) ? headers : Object.entries(headers ?? {});
  const auth = entries.filter(([key]) => key.toLowerCase() === 'authorization').map(([, value]) => String(value));
  if (method !== 'POST' && path !== '/api/v2/public/get_tradingview_chart_data') return false;
  if (path === '/api/v2/public/auth') {
    const wire = typeof body === 'string' ? body : new TextDecoder().decode(body);
    const rpc = JSON.parse(wire);
    return auth.length === 0 && rpc.params?.grant_type === 'coinbase_cdp'
      && verifyCoinbaseCdpJwt(rpc.params.token, { uri: 'POST drb.coinbase.com/api/v2/public/auth', now });
  }
  if (path.startsWith('/api/v2/private/')) return auth.length === 1 && auth[0] === `Bearer ${TOKEN}`;
  return auth.length === 0;
};

/** Socket requests are answered by method; the tape keeps only the server pushes. */
export const drbSocketReplies = (url, message) => {
  if (!isDrb(url) || message?.jsonrpc !== '2.0' || message.id === undefined) return [];
  const reply = (result) => [JSON.stringify({ jsonrpc: '2.0', id: message.id, result })];
  switch (message.method) {
    case 'public/auth':
      return reply({ access_token: SOCKET_TOKEN, expires_in: 900, scope: 'trade:read', token_type: 'bearer' });
    case 'private/subscribe':
    case 'public/subscribe':
    case 'public/unsubscribe':
      return reply(message.params?.channels ?? []);
    case 'public/set_heartbeat':
    case 'public/test':
      return reply('ok');
    default:
      return [];
  }
};

export const verifyDrbSocketMessage = (url, message, now = Date.now()) => {
  if (message?.jsonrpc !== '2.0' || !Number.isSafeInteger(message.id)) return false;
  if (message.method === 'public/auth')
    return url === DRB_PRIVATE_SOCKET && message.params?.grant_type === 'coinbase_cdp'
      && verifyCoinbaseCdpJwt(message.params.token, { now });
  if (message.method?.startsWith('private/')) return url === DRB_PRIVATE_SOCKET;
  return true;
};

/** Startup is ready once the private socket authenticated and holds every channel. */
export const createDrbSocketAudit = () => {
  const connections = new Map();
  const errors = [];
  const get = (url, generation = 1) => {
    const key = `${url} ${generation}`;
    if (!connections.has(key))
      connections.set(key, { url, authenticated: false, channels: new Set(), pending: new Map(), closed: false });
    return connections.get(key);
  };
  return {
    connected(url, generation) { get(url, generation); },
    closed(url, generation) { get(url, generation).closed = true; },
    sent(url, text, generation) {
      let message;
      try { message = JSON.parse(text); } catch { errors.push('Coinbase drb sent non-JSON'); return; }
      const state = get(url, generation);
      if (state.closed || !verifyDrbSocketMessage(url, message)) {
        errors.push(`Coinbase drb invalid socket request ${message?.method}`);
        return;
      }
      if (message.method?.startsWith('private/') && message.method !== 'private/subscribe')
        errors.push(`Coinbase drb unexpected socket method ${message.method}`);
      if (message.method === 'private/subscribe' && !state.authenticated) {
        errors.push('Coinbase drb subscribed before authenticating');
        return;
      }
      if (message.method === 'public/auth' || message.method === 'private/subscribe') {
        if (state.pending.has(message.id)) {
          errors.push('Coinbase drb reused a pending socket request ID');
          return;
        }
        state.pending.set(message.id, { method: message.method, channels: message.params?.channels });
      }
    },
    received(url, text, generation) {
      let message;
      try { message = JSON.parse(text); }
      catch { errors.push('Coinbase drb received invalid JSON'); return; }
      const state = get(url, generation);
      if (state.closed || message?.jsonrpc !== '2.0' || !Number.isSafeInteger(message.id)) return;
      const pending = state.pending.get(message.id);
      if (!pending) return;
      state.pending.delete(message.id);
      if (message.error) { errors.push('Coinbase drb socket returned an error'); return; }
      if (pending.method === 'public/auth') {
        if (typeof message.result?.access_token !== 'string' || !message.result.access_token
            || !Number.isFinite(message.result.expires_in) || message.result.expires_in <= 0) {
          errors.push('Coinbase drb invalid authentication acknowledgement');
          return;
        }
        state.authenticated = true;
      } else if (state.authenticated && Array.isArray(message.result) && Array.isArray(pending.channels)
          && pending.channels.every((channel) => message.result.includes(channel))) {
        for (const channel of pending.channels) state.channels.add(channel);
      } else {
        errors.push('Coinbase drb incomplete subscription acknowledgement');
      }
    },
    ready() {
      return [...connections.values()].some((state) => !state.closed && state.url === DRB_PRIVATE_SOCKET
        && state.authenticated && PRIVATE_CHANNELS.every((channel) => state.channels.has(channel)));
    },
    errors() { return [...errors, ...(this.ready() ? [] : ['Coinbase drb missing private socket subscriptions'])]; },
  };
};

export function coinbaseDrbIntxTape({ algorithm = 'EdDSA' } = {}) {
  const epoch = 1765805280000;
  const credentials = algorithm === 'ES256' ? COINBASE_ES256_REPLAY : COINBASE_REPLAY;
  const records = [
    { t: 0, type: 'meta', private: true, epoch, venue: 'coinbase', profile: 'intx', algorithm, oracle: DRB_RUST_ONLY },
    { t: 0, type: 'config', private: true, options: { ...credentials, spotOnly: false, testnet: false } },
  ];
  const rest = 'https://api.coinbase.com/api/v3/brokerage';
  const get = (url, body) => records.push({ t: 0, type: 'http', method: 'GET', url, status: 200, afterCommand: 0, body: JSON.stringify(body) });
  const rpc = (method, params, result, extra = {}) => records.push({
    t: 0, type: 'http', method: 'POST', url: `${DRB_HTTP}/${method}`, status: 200, afterCommand: 0,
    requestBody: JSON.stringify({ jsonrpc: '2.0', method, params }),
    body: JSON.stringify({ jsonrpc: '2.0', id: '$request:id', result }), ...extra,
  });
  let clock = 0;
  const push = (channel, data) => records.push({
    t: (clock += 200), type: 'ws', url: DRB_PRIVATE_SOCKET,
    text: JSON.stringify({ jsonrpc: '2.0', method: 'subscription', params: { channel, data } }),
  });

  // Portfolio discovery still identifies the INTX account on Advanced Trade.
  const portfolio = { uuid: 'replay-portfolio', name: 'Replay', type: 'INTX', deleted: false };
  get(`${rest}/portfolios`, { portfolios: [portfolio] });
  get(`${rest}/portfolios/replay-portfolio`, { breakdown: { portfolio, spot_positions: [] } });

  rpc('public/auth', { grant_type: 'coinbase_cdp', token: '<verified-jwt>' },
    { access_token: TOKEN, expires_in: 900, scope: 'trade:read', token_type: 'bearer' });
  rpc('public/get_instruments', { currency: 'USDC', kind: 'future', expired: false }, [{
    instrument_name: INSTRUMENT, kind: 'future', instrument_type: 'linear', settlement_period: 'perpetual',
    settlement_currency: 'USDC', base_currency: 'BTC', quote_currency: 'USDC', is_active: true,
    tick_size: 0.1, contract_size: 0.0001, min_trade_amount: 0.0001, max_leverage: 50,
  }]);
  rpc('public/get_book_summary_by_currency', { currency: 'USDC', kind: 'future' }, [{
    instrument_name: INSTRUMENT, funding_8h: 0.0001, last: 110000, mark_price: 110000,
    estimated_delivery_price: 110005, volume: 2, volume_notional: 220000, open_interest: 30,
    bid_price: 109990, ask_price: 110010,
  }]);
  rpc('private/get_account_summary', { currency: 'USDC', extended: true }, {
    currency: 'USDC', balance: 1000, equity: 1020, available_funds: 800, initial_margin: 220,
    maintenance_margin: 100, futures_session_upl: 20, margin_model: 'cross_sm',
  });
  rpc('private/get_positions', { currency: 'USDC', kind: 'future', include_isolated: true }, [{
    instrument_name: INSTRUMENT, kind: 'future', direction: 'buy', size: 11000, size_currency: 0.1,
    average_price: 100000, mark_price: 110000, index_price: 110005, floating_profit_loss: 1000,
    realized_profit_loss: 5, initial_margin: 1100, maintenance_margin: 550, open_orders_margin: 0,
    leverage: 10, estimated_liquidation_price: 60000,
  }]);
  const open = {
    order_id: 'USDC-1001', instrument_name: INSTRUMENT, direction: 'buy', order_state: 'open',
    order_type: 'limit', time_in_force: 'good_til_cancelled', price: 100000, amount: 0.02,
    filled_amount: 0, label: 'replay-entry', post_only: true, reduce_only: false,
    creation_timestamp: epoch - 60000, last_update_timestamp: epoch - 60000,
  };
  rpc('private/get_open_orders', { kind: 'future', include_isolated: true }, [open]);

  // After the subscription is confirmed, the stream fills half of the resting
  // order and then reports the new portfolio balance.
  push(PRIVATE_CHANNELS[0], {
    orders: [{ ...open, order_state: 'open', filled_amount: 0.01, last_update_timestamp: epoch + 1000 }],
    trades: [{
      trade_id: 'USDC-T1', trade_seq: 1, order_id: 'USDC-1001', instrument_name: INSTRUMENT,
      timestamp: epoch + 1000, direction: 'buy', price: 100000, amount: 0.01, fee: 0.2,
      fee_currency: 'USDC', liquidity: 'M',
    }],
    positions: [],
  });
  push(PRIVATE_CHANNELS[1], {
    currency: 'USDC', balance: 999.8, equity: 1019.8, available_funds: 690, initial_margin: 329.8,
    maintenance_margin: 160, futures_session_upl: 20, margin_model: 'cross_sm',
  });
  // Expected Store state, from the documented field mapping (see private_drb.rs):
  // balance total=equity, free=available_funds, used=total-free, upnl=session UPL;
  // linear size in base units (size_currency); notional = size x mark; margin =
  // initial margin. The stream fill leaves the order partially filled.
  const total = 1019.8, free = 690;
  records[0].expected = {
    commands: [],
    account: {
      hedged: false,
      balance: { used: total - free, free, total, upnl: 20 },
      orders: [{
        id: 'USDC-1001', clientOrderId: 'replay-entry', symbol: INSTRUMENT, status: 'open', type: 'limit',
        side: 'buy', price: 100000, takeProfit: null, stopLoss: null, conditionalOrderPrice: null, parentId: null,
        amount: 0.02, filled: 0.01, timeInForce: 'PostOnly', remaining: 0.01, reduceOnly: false, close: false,
        trigger: null, takeProfitTrigger: null, stopLossTrigger: null, rejectReason: null,
      }],
      positions: [{
        symbol: INSTRUMENT, side: 'long', entryPrice: 100000, contracts: 0.1, notional: 0.1 * 110000,
        leverage: 10, unrealizedPnl: 1000, realizedPnl: 5, liquidationPrice: 60000, marginType: 'cross',
        positionMode: 'oneway', marginSize: 1100,
      }],
      flatPositionPnl: [],
      settings: [{ symbol: INSTRUMENT, marginType: 'cross', leverage: 10 }],
      fills: [{
        id: 'USDC-1001', symbol: INSTRUMENT, side: 'buy', amount: 0.01, price: 100000,
        executionId: 'USDC-T1', fee: 0.2, timestamp: null,
      }],
    },
  };
  return records;
}
