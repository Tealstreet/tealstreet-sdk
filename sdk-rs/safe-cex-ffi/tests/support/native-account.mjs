import { createHash } from 'node:crypto';
import { createServer } from 'node:http';
import { createRequire } from 'node:module';

export const native = createRequire(import.meta.url)('../../dist/node/safe_cex_ffi.node');
export const orderOptions = { symbol: 'BTCUSDT', side: 'buy', type: 'limit', amount: 1, price: 10 };
const market = { id: 'BTCUSDT', symbol: 'BTCUSDT', base: 'BTC', quote: 'USDT', active: true,
  precision: { amount: 0.001, price: 0.1 }, limits: { amount: { min: 0.001, max: 100 }, leverage: { min: 1, max: 20 } } };
const ticker = { id: 'BTCUSDT', symbol: 'BTCUSDT', bid: 9.9, ask: 10.1, last: 10, mark: 10, index: 10,
  percentage: 0, openInterest: 0, fundingRate: 0, fundingTime: 0, volume: 1, quoteVolume: 10 };
const order = { orderId: 77, clientOrderId: 'original', symbol: 'BTCUSDT', side: 'BUY', type: 'LIMIT', origQty: '1', price: '10', stopPrice: '0', status: 'NEW', executedQty: '0', reduceOnly: false, positionSide: 'BOTH', time: 1 };

// Minimal server-to-client text frame for a loopback-only public-data fixture.
function textFrame(value) {
  const body = Buffer.from(JSON.stringify(value));
  const header = Buffer.alloc(body.length < 126 ? 2 : 4);
  header[0] = 0x81;
  if (body.length < 126) header[1] = body.length;
  else { header[1] = 126; header.writeUInt16BE(body.length, 2); }
  return Buffer.concat([header, body]);
}
export async function openStartedAccount(t) {
  const seen = [];
  const sockets = new Set();
  const server = createServer(async (request, response) => {
    const url = new URL(request.url.replace(/^\/https?:\/\/[^/]+/, ''), 'http://local');
    let body = '';
    for await (const chunk of request) body += chunk;
    seen.push({ method: request.method, path: url.pathname, params: url.searchParams, body });
    const routes = {
      'POST /fapi/v1/order': { orderId: 9001 },
      'GET /fapi/v1/openOrders': [order],
      'GET /fapi/v1/openAlgoOrders': [],
      'DELETE /fapi/v1/allOpenOrders': {},
      'DELETE /fapi/v1/algoOpenOrders': {},
    };
    const answer = routes[`${request.method} ${url.pathname}`];
    response.writeHead(answer ? 200 : 404, { 'content-type': 'application/json' });
    response.end(JSON.stringify(answer ?? { code: -1, msg: 'unexpected test route' }));
  });
  server.on('upgrade', (request, socket) => {
    sockets.add(socket);
    socket.on('error', () => {});
    socket.on('close', () => sockets.delete(socket));
    const accept = createHash('sha1').update(request.headers['sec-websocket-key'] + '258EAFA5-E914-47DA-95CA-C5AB0DC85B11').digest('base64');
    socket.write(`HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: ${accept}\r\n\r\n`);
    // Once the provider writes subscriptions, supply both startup snapshots.
    // Resending is harmless: the provider replaces its market/ticker batches.
    socket.on('data', () => {
      if (socket.destroyed) return;
      for (const [channel, data] of [['market', market], ['tickers', ticker]]) {
        socket.write(textFrame({ type: 'data', exchangeId: 'binance', channel, data: [data] }));
      }
    });
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  const origin = `127.0.0.1:${server.address().port}`;
  let id;
  t.after(async () => {
    try { if (id !== undefined) await native.nativeAccountClose(id); }
    finally {
      for (const socket of sockets) socket.destroy();
      server.closeAllConnections();
      await new Promise(resolve => server.close(resolve));
    }
  });
  const events = [];
  const stores = [];
  id = await native.nativeAccountOpen({ exchange: 'binance', optionsJson: JSON.stringify({
    key: 'test-key', secret: 'test-secret', publicDataOnly: true, overrideProxyUrl: `http://${origin}`,
    externalPublicData: { type: 'websocket', url: `ws://${origin}/gateway`, exchangeId: 'binance' },
  }) }, event => events.push(JSON.parse(event)), store => stores.push(JSON.parse(store)));
  const call = async (method, ...args) => JSON.parse(await native.nativeAccountCallMethod(id, method, JSON.stringify(args)));
  await call('start');
  return { call, seen, events, stores };
}
