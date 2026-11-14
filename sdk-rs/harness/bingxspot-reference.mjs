// Fixture adapter for the independent Spot oracle; never imported by SDK factories.
import axios from 'axios';
import { bigIntAsStringJsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { TinyEmitter } from 'tiny-emitter';
import { socketMessages } from '../hosts/wasm/js/host.mjs';
import { BingxSpotReference, SPOT_WS, spotSigningGolden } from '../../packages/safe-cex/src/__tests__/reference/bingxspot/bingxspot.exchange.ts';

export class BingxSpotReplayExchange {
  constructor(_name, options, store) {
    this.options = options;
    this.store = store;
    this.emitter = new TinyEmitter();
    this.xhr = axios.create({ baseURL: 'https://open-api.bingx.com', transformResponse: [value => value] });
    this.oracle = new BingxSpotReference((method, path, params) => this.request(method, path, params));
    this.books = new Map();
    this.seenFills = new Set();
    this.pending = new Set();
    this.publicWebsocket = { messageHandlers: {} };
  }
  async request(method, path, params = {}) {
    const signed = !['/common/symbols', '/quote/ticker', '/quote/klines', '/market/depth'].some(p => path.endsWith(p));
    const fields = signed && !path.endsWith('/userDataStream')
      ? JSON.parse(spotSigningGolden(params, Date.now(), this.options.secret).body) : params;
    const response = await this.xhr.request({ method, url: path,
      headers: signed ? { 'X-BX-APIKEY': this.options.key } : {},
      ...(method === 'GET' ? { params: fields } : { data: Object.keys(fields).length ? JSON.stringify(fields) : undefined }),
    });
    return response.data;
  }
  async start() {
    const markets = await this.oracle.fetchMarkets();
    this.store.update({ markets, loaded: { ...this.store.loaded, markets: true } });
    const tickers = await this.oracle.fetchTickers();
    this.store.update({ tickers, loaded: { ...this.store.loaded, tickers: true } });
    this.public = this.socket(SPOT_WS, markets.map(m => `${m.id}@ticker`));
    if (!this.options.publicDataOnly) {
      const session = JSON.parse(await this.request('POST', '/openApi/user/auth/userDataStream'));
      if (!session.listenKey) throw new Error('Spot fixture listen key missing');
      this.private = this.socket(`${SPOT_WS}?listenKey=${session.listenKey}`, ['spot.executionReport', 'ACCOUNT_UPDATE']);
      await this.account();
    } else {
      this.store.update({ loaded: { ...this.store.loaded, balance: true, orders: true, positions: true } });
    }
  }
  async account() {
    const [balance, orders] = await Promise.all([this.oracle.fetchBalance(), this.oracle.fetchOrders()]);
    this.store.update({ balance, orders, positions: [], loaded: { ...this.store.loaded, balance: true, orders: true, positions: true } });
  }
  track(promise) {
    this.pending.add(promise);
    promise.then(() => this.pending.delete(promise), error => { this.pending.delete(promise); this.failure = error; });
    return promise;
  }
  async settle() { while (this.pending.size) await Promise.all(this.pending); if (this.failure) throw this.failure; }
  subscribe(socket, channel) {
    const send = () => socket.send(JSON.stringify({ id: channel, reqType: 'sub', dataType: channel }));
    if (socket.readyState === 1) send();
    else socket.addEventListener('open', send);
  }
  socket(url, channels) {
    const socket = new WebSocket(url);
    for (const channel of channels) this.subscribe(socket, channel);
    const receive = socketMessages(value => this.message(socket, value), error => { this.failure = error; });
    socket.addEventListener('message', event => receive(event.data));
    return socket;
  }
  message(socket, value) {
    if (value === 'Ping') { socket.send('Pong'); return; }
    if (value === 'Pong') return;
    const { code = 0, dataType, data } = bigIntAsStringJsonParse(value);
    if (code !== 0 || !data) return;
    if (dataType.endsWith('@ticker')) {
      this.store.updateTicker({ id: data.s }, { last: Number(data.c), percentage: Number(data.P),
        bid: Number(data.B), ask: Number(data.A), volume: Number(data.v), quoteVolume: Number(data.q) });
    } else if (dataType.endsWith('@depth50')) {
      const symbol = dataType.split('@')[0].replaceAll('-', '');
      const levels = rows => rows.map(([price, amount]) => [Number(price), Number(amount)]);
      this.books.get(symbol)?.({ bids: levels(data.bids).sort((a, b) => b[0] - a[0]).map(([price, amount]) => ({ price, amount })),
        asks: levels(data.asks).sort((a, b) => a[0] - b[0]).map(([price, amount]) => ({ price, amount })) });
    } else if (dataType === 'spot.executionReport') {
      const order = this.oracle.mapOrder({ symbol: data.s, orderId: data.i, clientOrderID: data.C,
        side: data.S, type: data.o, status: data.X, origQty: data.q, executedQty: data.z, price: data.p,
        updateTime: data.T });
      if (order.status === 'open') this.store.addOrUpdateOrder(order);
      else this.store.removeOrder(order);
      const fillKey = `${data.i}:${data.t}`;
      if (Number(data.l) > 0 && Number(data.L) > 0 && !this.seenFills.has(fillKey)) {
        this.seenFills.add(fillKey);
        this.emitter.emit('fill', { id: String(data.t), orderId: String(data.i), symbol: order.symbol,
          executionId: String(data.t), side: order.side, price: Number(data.L), amount: Number(data.l), fee: Number(data.n) });
      }
      this.track(this.oracle.fetchBalance().then(balance => this.store.update({ balance })));
    } else if (dataType === 'ACCOUNT_UPDATE') {
      this.track(this.oracle.fetchBalance().then(balance => this.store.update({ balance })));
    }
  }
  listenOrderBook(symbol, callback) {
    this.books.set(symbol, callback);
    const channel = `${this.oracle.markets.find(m => m.symbol === symbol).id}@depth50`;
    this.publicWebsocket.messageHandlers[channel] = callback;
    this.subscribe(this.public, channel);
    return () => this.books.delete(symbol);
  }
  async placeOrders(orders) {
    const ids = [];
    for (const order of orders) ids.push(...await this.oracle.placeOrder(order));
    return ids;
  }
  async cancelOrders(orders) { for (const order of orders) { await this.oracle.cancelOrder(order.id, order.symbol); this.store.removeOrder(order); } }
  fetchHistoricalFills(options) { return this.oracle.fetchHistoricalFills(options); }
  fetchOHLCV(options) { return this.oracle.fetchOHLCV(options); }
  fetchOrderBook(symbol) { return this.oracle.fetchOrderBook(symbol); }
  dispose() { this.public?.close(); this.private?.close(); }
}
