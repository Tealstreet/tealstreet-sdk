// A fake Binance USD-M venue for driving a real wasm engine through EngineHost:
// documented startup payloads (as `sdk-rs/harness/private-tape.mjs`), placements
// acknowledged on the user stream, cancels, and fills the test pushes.
export const LISTEN_KEY = 'fixture-listen-key';
const filters = [
  { filterType: 'PRICE_FILTER', tickSize: '0.10', minPrice: '0.10', maxPrice: '4529764' },
  { filterType: 'LOT_SIZE', stepSize: '0.001', minQty: '0.001', maxQty: '1000' },
  { filterType: 'MARKET_LOT_SIZE', stepSize: '0.001', minQty: '0.001', maxQty: '120' },
];
const STARTUP = {
  '/fapi/v1/exchangeInfo': { timezone: 'UTC', symbols: [{ symbol: 'BTCUSDT', status: 'TRADING', baseAsset: 'BTC',
    quoteAsset: 'USDT', marginAsset: 'USDT', contractType: 'PERPETUAL', filters }] },
  '/fapi/v1/leverageBracket': [{ symbol: 'BTCUSDT', brackets: [{ bracket: 1, initialLeverage: 125, notionalCap: 50000,
    notionalFloor: 0, maintMarginRatio: 0.004, cum: 0 }] }],
  '/fapi/v1/ticker/24hr': [{ symbol: 'BTCUSDT', lastPrice: '96000.0', priceChangePercent: '1.2', volume: '1000', quoteVolume: '96000000' }],
  '/fapi/v1/ticker/bookTicker': [{ symbol: 'BTCUSDT', bidPrice: '95999.9', askPrice: '96000.0' }],
  '/fapi/v1/premiumIndex': [{ symbol: 'BTCUSDT', markPrice: '96000.0', indexPrice: '95990.0', lastFundingRate: '0.0001',
    nextFundingTime: 1760889600000 }],
  '/fapi/v1/listenKey': { listenKey: LISTEN_KEY },
  '/fapi/v1/positionSide/dual': { dualSidePosition: false },
  '/fapi/v3/positionRisk': [],
  '/fapi/v1/symbolConfig': [{ symbol: 'BTCUSDT', marginType: 'CROSSED', isAutoAddMargin: false, leverage: 20, maxNotionalValue: '1000000' }],
  '/fapi/v3/account': { totalInitialMargin: '0', totalMaintMargin: '0', totalWalletBalance: '5000', totalUnrealizedProfit: '0',
    totalMarginBalance: '5000', availableBalance: '5000', maxWithdrawAmount: '5000', assets: [{ asset: 'USDT', walletBalance: '5000',
      unrealizedProfit: '0', marginBalance: '5000', maintMargin: '0', initialMargin: '0', availableBalance: '5000', updateTime: 1760880000000 }],
    positions: [] },
  '/fapi/v1/openOrders': [],
  '/fapi/v1/openAlgoOrders': [],
};

/** A Binance USD-M venue: startup reads, order placement and cancels over HTTP,
 * user-stream events pushed by the test. Its timers run on a virtual clock that
 * `advance` moves, or on the global (possibly faked) timers with `clock: 'global'`. */
export function binanceVenue({ clock = 'virtual' } = {}) {
  let virtualNow = 1_760_880_000_000;
  const now = () => (clock === 'global' ? Date.now() : virtualNow);
  let seq = 0;
  const timers = new Map();
  const sockets = [];
  const placements = [];
  const cancels = [];
  const unknown = new Set();
  let nextOrder = 5000;
  const respond = (packet) => {
    const url = new URL(packet.url);
    const params = new URLSearchParams(packet.body ?? url.search);
    const json = (body, status = 200) => Promise.resolve({ status, headers: 'content-type: application/json', body: new TextEncoder().encode(JSON.stringify(body)) });
    if (url.pathname === '/fapi/v1/order' && packet.method === 'POST') {
      const order = { orderId: ++nextOrder, symbol: params.get('symbol'), status: 'NEW', clientOrderId: params.get('newClientOrderId'),
        side: params.get('side'), type: params.get('type'), origQty: params.get('quantity'), price: params.get('price') ?? '0',
        executedQty: '0', cumQuote: '0', avgPrice: '0', timeInForce: params.get('timeInForce') ?? 'GTC', reduceOnly: false,
        positionSide: 'BOTH', updateTime: now() };
      placements.push(order);
      // Binance's user stream acknowledges every placement.
      setImmediate(() => orderUpdate(order, {}));
      return json(order);
    }
    if (url.pathname === '/fapi/v1/order' && packet.method === 'DELETE') {
      const order = placements.find((placed) => String(placed.orderId) === params.get('orderId')
        || placed.clientOrderId === params.get('origClientOrderId'));
      cancels.push(String(order?.orderId));
      if (!order || order.status !== 'NEW') return json({ code: -2011, msg: 'Unknown order sent.' }, 400);
      order.status = 'CANCELED';
      setImmediate(() => orderUpdate(order, { x: 'CANCELED', X: 'CANCELED' }));
      return json(order);
    }
    if (Object.hasOwn(STARTUP, url.pathname)) return json(STARTUP[url.pathname]);
    unknown.add(`${packet.method} ${url.pathname}`);
    return json({});
  };
  const transport = {
    now,
    connect(url, handlers) {
      const socket = { url, handlers, sent: [], closed: false };
      sockets.push(socket);
      setImmediate(() => { if (!socket.closed) handlers.onOpen(); });
      return {
        send(text) {
          socket.sent.push(text);
          const message = JSON.parse(text);
          if (message.id !== undefined) setImmediate(() => handlers.onMessage(JSON.stringify({ result: null, id: message.id })));
        },
        close() { socket.closed = true; },
      };
    },
    prepareHttp: (request) => ({ bind: (packet) => ({ ready: () => true, dispatch: () => respond(packet) }) }),
    request: (packet) => respond(packet),
    setTimeout(fn, ms) {
      if (clock === 'global') return setTimeout(fn, ms);
      const handle = ++seq;
      timers.set(handle, { at: virtualNow + Math.max(0, ms), fn });
      return handle;
    },
    clearTimeout(handle) { if (clock === 'global') clearTimeout(handle); else timers.delete(handle); },
  };
  const settle = async () => { for (let i = 0; i < 20; i++) await new Promise((resolve) => setImmediate(resolve)); };
  /** Fire every due virtual timer in time order, settling the I/O each one starts. */
  const advance = async (ms) => {
    const until = virtualNow + ms;
    for (;;) {
      let next;
      for (const [handle, timer] of timers) if (timer.at <= until && (!next || timer.at < next[1].at)) next = [handle, timer];
      if (!next) break;
      timers.delete(next[0]);
      virtualNow = Math.max(virtualNow, next[1].at);
      next[1].fn();
      await settle();
    }
    virtualNow = until;
    await settle();
  };
  const privateSocket = () => sockets.find((socket) => socket.url.includes(`listenKey=${LISTEN_KEY}`) && !socket.closed);
  const orderUpdate = (order, fields) => privateSocket().handlers.onMessage(JSON.stringify({
    e: 'ORDER_TRADE_UPDATE', E: now(), T: now(),
    o: { s: order.symbol, c: order.clientOrderId, S: order.side, o: order.type, f: order.timeInForce, q: order.origQty, p: order.price,
      ap: '0', sp: '0', x: 'NEW', X: 'NEW', i: order.orderId, l: '0', z: '0', L: '0', n: '0', N: 'USDT', T: now(), t: 0, b: '0', a: '0',
      m: true, R: false, wt: 'CONTRACT_PRICE', ot: order.type, ps: 'BOTH', cp: false, rp: '0', pP: false, si: 0, ss: 0, V: 'NONE',
      pm: 'NONE', gtd: 0, ...fields },
  }));
  /** The user stream reports `order` filled in one trade. */
  const fill = (order) => (order.status = 'FILLED', orderUpdate(order, { x: 'TRADE', X: 'FILLED', ap: order.price, l: order.origQty, z: order.origQty,
    L: order.price, t: 70000 + order.orderId }));
  return { transport, placements, cancels, unknown, settle, advance, fill, privateSocket, timers };
}

