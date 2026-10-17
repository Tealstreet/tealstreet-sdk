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
export async function openStartedAccount(t, options = {}) {
  const seen = [];
  const sockets = new Set();
  const received = [];
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
    let buffered = Buffer.alloc(0);
    socket.on('data', chunk => {
      buffered = Buffer.concat([buffered, chunk]);
      while (buffered.length >= 2) {
        const opcode = buffered[0] & 15;
        let length = buffered[1] & 127;
        let offset = 2;
        if (length === 126) {
          if (buffered.length < 4) return;
          length = buffered.readUInt16BE(2); offset = 4;
        }
        if (length === 127) throw new Error('unexpected oversized fixture frame');
        const masked = (buffered[1] & 128) !== 0;
        const maskBytes = masked ? 4 : 0;
        if (buffered.length < offset + maskBytes + length) return;
        const mask = buffered.subarray(offset, offset + maskBytes);
        const payload = Buffer.from(buffered.subarray(offset + maskBytes, offset + maskBytes + length));
        if (masked) for (let i = 0; i < payload.length; i++) payload[i] ^= mask[i % 4];
        buffered = buffered.subarray(offset + maskBytes + length);
        if (opcode === 8) { socket.end(Buffer.from([0x88, 0])); return; }
        if (opcode !== 1) continue;
        const frame = JSON.parse(payload.toString());
        received.push(frame);
        if (frame.type === 'subscribe' && ['market', 'tickers'].includes(frame.channel)) {
          socket.write(textFrame({ type: 'data', exchangeId: 'binance', channel: frame.channel,
            data: [frame.channel === 'market' ? market : ticker] }));
        }
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
  id = await native.nativeAccountOpen({ exchange: 'binance', options: {
    key: 'test-key', secret: 'test-secret', publicDataOnly: true, overrideProxyUrl: `http://${origin}`,
    ...options,
    externalPublicData: { type: 'websocket', url: `ws://${origin}/gateway`, exchangeId: 'binance' },
  } }, event => events.push(event), store => stores.push(store));
  const call = async (method, ...args) => await native.nativeAccountCallMethod(id, method, args);
  await call('start');
  return { call, seen, events, stores, received, close: async () => {
    await native.nativeAccountClose(id);
    id = undefined;
  }, sendGateway: frame => {
    for (const socket of sockets) if (!socket.destroyed) socket.write(textFrame(frame));
  } };
}
