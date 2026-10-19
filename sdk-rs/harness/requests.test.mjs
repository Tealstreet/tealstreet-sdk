import assert from 'node:assert/strict';
import { test } from 'node:test';
import { requestKey, takeResponse, subscriptionKeys } from './requests.mjs';
const url = 'https://open-api.bingx.com/openApi/swap/v2/trade/order';

test('write matching preserves numeric/body types and exact caller IDs', () => {
  const ids = new Set(['caller-lowercase']);
  const key = (body) => requestKey('POST', url, body, ids);
  assert.notEqual(key({ quantity: 1 }), key({ quantity: '1' }));
  assert.notEqual(key({ takeProfit: { stopPrice: 1 } }), key({ takeProfit: '{"stopPrice":1}' }));
  assert.notEqual(key({ clientOrderId: 'caller-lowercase' }), key({ clientOrderId: 'generated' }));
  assert.equal(key({ clientOrderId: 'tsgenerated1', timestamp: 1, quantity: 1 }), key({ clientOrderId: 'tsgenerated2', timestamp: 2, quantity: 1 }));
});
test('a single recorded write cannot serve duplicate submissions', () => {
  const response = { status: 200, body: '{}' };
  for (const method of ['POST', 'PUT', 'DELETE']) {
    const queue = [response];
    assert.equal(takeResponse(queue, method, url), response);
    assert.equal(takeResponse(queue, method, url), undefined);
  }
});
test('polling and listen-key reads can reuse responses; snapshots can change with tape time', () => {
  const first = { body: 'first' };
  const next = { body: 'next', availableAfter: 1000 };
  const queue = [first, next];
  assert.equal(takeResponse(queue, 'GET', url, 999), first);
  assert.equal(takeResponse(queue, 'GET', url, 1000), next);
  const auth = [first];
  const path = 'https://open-api.bingx.com/openApi/user/auth/userDataStream';
  assert.equal(takeResponse(auth, 'POST', path), first);
  assert.equal(takeResponse(auth, 'PUT', path), first);
  assert.equal(auth.length, 1);
});
test('Binance query matching preserves a caller ID while ignoring generated IDs', () => {
  const root = 'https://fapi.binance.com/fapi/v1/order';
  const ids = new Set(['caller']);
  assert.equal(requestKey('POST', `${root}?newClientOrderId=x&timestamp=1`), requestKey('POST', `${root}?newClientOrderId=y&timestamp=2`));
  assert.notEqual(requestKey('POST', `${root}?newClientOrderId=caller`, undefined, ids), requestKey('POST', `${root}?newClientOrderId=changed`, undefined, ids));
});

test('nested map order is ignored and unsafe numeric order IDs remain exact', () => {
  assert.equal(requestKey('POST',url,{stop:{price:1,quantity:2}}),requestKey('POST',url,{stop:{quantity:2,price:1}}));
  assert.notEqual(requestKey('POST',url,'{"cancelOrderId":8389765519993909000}'),requestKey('POST',url,'{"cancelOrderId":8389765519993909001}'));
  assert.notEqual(requestKey('POST',url,'{"cancelOrderId":8389765519993909000}'),requestKey('POST',url,'{"cancelOrderId":"8389765519993909000"}'));
});

test('subscription matching ignores request IDs and batching, retaining native topics', () => {
  const socket='wss://ws-market-swap.qq-os.com/ws';
  assert.deepEqual(subscriptionKeys(socket,'{"id":"random","reqType":"sub","dataType":"market.contracts"}'),['wss://ws-market-swap.qq-os.com/ws subscribe market.contracts']);
  assert.deepEqual(subscriptionKeys(socket,'Ping'),[]);
  assert.deepEqual(subscriptionKeys(socket,'{"pong":1}'),[]);
  assert.deepEqual(subscriptionKeys('wss://fstream.binance.com/public/ws','{"method":"SUBSCRIBE","params":["btcusdt@depth@100ms","ethusdt@depth@100ms"],"id":1}'),[
    'wss://fstream.binance.com/public/ws subscribe btcusdt@depth@100ms',
    'wss://fstream.binance.com/public/ws subscribe ethusdt@depth@100ms',
  ]);
});

test('Bybit connection IDs vary while body types and subscription topics stay exact', () => {
  const bytes = new TextEncoder().encode('{"category":"linear","qty":"1","requestId":"fresh"}');
  assert.equal(requestKey('POST',url,bytes), requestKey('POST',url,{qty:'1',category:'linear',requestId:'other'}));
  assert.notEqual(requestKey('POST',url,bytes), requestKey('POST',url,{qty:1,category:'linear'}));
  assert.deepEqual(subscriptionKeys('wss://stream.bybit.com/v5/private?requestId=fresh','{"op":"subscribe","args":["order","execution"]}'),[
    'wss://stream.bybit.com/v5/private subscribe order',
    'wss://stream.bybit.com/v5/private subscribe execution',
  ]);
});

test('command and time stages cannot serve future snapshots or reuse writes', () => {
  const initial={method:'GET',url,body:'initial'};
  const staged={method:'GET',url,body:'next',afterCommand:2,availableAfter:100};
  const queue=[initial,staged];
  assert.equal(takeResponse(queue,'GET',url,200,1),initial);
  assert.equal(takeResponse(queue,'GET',url,99,2),initial);
  assert.equal(takeResponse(queue,'GET',url,100,2),staged);
  const write={method:'POST',url,afterCommand:2};
  const writes=[write];
  assert.equal(takeResponse(writes,'POST',url,0,1),undefined);
  assert.equal(takeResponse(writes,'POST',url,0,2),write);
  assert.equal(takeResponse(writes,'POST',url,0,2),undefined);
});

test('nested generated OKX IDs vary while caller IDs and body types stay exact', () => {
  const opts={generatedBodyId:(key,value)=>key==='clOrdId' && value.startsWith('broker')};
  const body=(id,sz='1')=>[{clOrdId:id,sz}];
  assert.equal(requestKey('POST',url,body('broker1'),new Set(),opts),requestKey('POST',url,body('broker2'),new Set(),opts));
  assert.notEqual(requestKey('POST',url,body('broker1'),new Set(['broker1']),opts),requestKey('POST',url,body('broker2'),new Set(['broker1']),opts));
  assert.notEqual(requestKey('POST',url,body('broker1'),new Set(),opts),requestKey('POST',url,body('broker2',1),new Set(),opts));
});

test('Crypto signing envelope varies while typed params and caller IDs remain exact', async () => {
  const { venueOf } = await import('./venues.mjs');
  const venue = venueOf('cryptocom');
  const route = 'https://api.crypto.com/exchange/v1/private/create-order';
  const key = (params, id = 1) => requestKey('POST', route, JSON.stringify({ id, nonce: id, sig: String(id), params }), new Set(['caller']), venue);
  assert.equal(key({ quantity: '1', client_oid: 'caller' }, 1), key({ client_oid: 'caller', quantity: '1' }, 2));
  assert.notEqual(key({ quantity: '1', client_oid: 'caller' }), key({ quantity: 1, client_oid: 'caller' }));
  assert.notEqual(key({ client_oid: 'caller' }), key({ client_oid: 'other' }));
  assert.notEqual(requestKey('POST', route, '[1]'), requestKey('POST', route, '{"0":1}'));
  assert.deepEqual(subscriptionKeys('wss://stream.crypto.com/exchange/v1/market', '{"method":"subscribe","params":{"channels":["ticker.BTCUSD-PERP"]}}'), [
    'wss://stream.crypto.com/exchange/v1/market subscribe ticker.BTCUSD-PERP',
  ]);
});
