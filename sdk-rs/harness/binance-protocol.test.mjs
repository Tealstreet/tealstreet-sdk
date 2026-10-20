import assert from 'node:assert/strict';
import test from 'node:test';
import { createHmac } from 'node:crypto';
import { validateBinanceRequest, createBinanceSpotSocketAudit, prepareBinanceSpotPublic, prepareBinanceSpotPrivate, fixBinanceSpotFills, binanceTickerSymbols, fixBinanceFilledStatus } from './binance-protocol.mjs';
import { Tape } from './tape.mjs';
import { subscriptionKeys } from './requests.mjs';
import { venueOf } from './venues.mjs';
const hmac = (text) => createHmac('sha256', 'replay-secret').update(text).digest('hex');
const URL = 'wss://ws-api.binance.com/ws-api/v3';
const subscribe = (id = 'subscribe') => {
  const timestamp = Date.now();
  return { id, method: 'userDataStream.subscribe.signature', params: {
    apiKey: 'replay-key', recvWindow: 5000, timestamp,
    signature: hmac(`apiKey=replay-key&recvWindow=5000&timestamp=${timestamp}`),
  } };
};
const send = (audit, message, generation = 1) => audit.sent(URL, JSON.stringify(message), generation);
const receive = (audit, message, generation = 1) => audit.received(URL, JSON.stringify(message), generation);
const ack = (audit, generation = 1) => receive(audit, { id: 'subscribe', status: 200, result: { subscriptionId: 7 } }, generation);

test('Binance REST verifies original bytes, header, clock and unsigned public/listen-key boundary', () => {
  const query = `symbol=BTCUSD_PERP&quantity=7&timestamp=${Date.now()}&recvWindow=20000`;
  const request = { method: 'POST', url: `https://dapi.binance.com/dapi/v1/order?${query}&signature=${hmac(query)}`, headers: {'X-MBX-APIKEY':'replay-key'} };
  assert.doesNotThrow(() => validateBinanceRequest(request));
  for (const changed of [
    {...request, url: request.url.replace('quantity=7', 'quantity=8')},
    {...request, headers: {}}, {...request, body: 'quantity=7'},
    {...request, url: request.url + '&timestamp=1'},
  ]) assert.throws(() => validateBinanceRequest(changed));
  assert.doesNotThrow(() => validateBinanceRequest({method:'GET', url:'https://api.binance.com/api/v3/exchangeInfo'}));
  assert.throws(() => validateBinanceRequest({method:'GET', url:'https://api.binance.com/api/v3/depth', headers:request.headers}));
  assert.doesNotThrow(() => validateBinanceRequest({method:'POST',url:'https://papi.binance.com/papi/v1/listenKey', headers:request.headers}));
  assert.throws(() => validateBinanceRequest({...request,url:'https://papi.binance.com/papi/v1/listenKey?signature=bad'}));
});

test('Binance testnet listen-key requests require fresh signed bytes while production stays unsigned', () => {
  const profiles = [
    ['https://demo-fapi.binance.com/fapi/v1/listenKey', 'https://fapi.binance.com/fapi/v1/listenKey'],
    ['https://demo-fapi.binance.com/papi/v1/listenKey', 'https://papi.binance.com/papi/v1/listenKey'],
    ['https://demo-dapi.binance.com/dapi/v1/listenKey', 'https://dapi.binance.com/dapi/v1/listenKey'],
  ];
  const headers = { 'X-MBX-APIKEY': 'replay-key' };
  for (const [testnet, production] of profiles) for (const method of ['POST', 'PUT']) {
    const signed = (query) => ({ method, headers, url: `${testnet}?${query}&signature=${hmac(query)}` });
    const query = `timestamp=${Date.now()}&recvWindow=20000`;
    const request = signed(query);
    assert.doesNotThrow(() => validateBinanceRequest(request));
    for (const bad of [
      { ...request, url: `${testnet}?${query}&signature=${'0'.repeat(64)}` },
      { ...request, url: `${testnet}?${query}` },
      signed(`timestamp=${Date.now() - 60001}&recvWindow=20000`),
      signed(`timestamp=${Date.now()}&recvWindow=60001`),
      signed(`${query}&timestamp=${Date.now()}`),
      { ...request, headers: {} },
      { method, headers, url: testnet },
      { ...request, url: request.url.replace(testnet, production) },
    ]) assert.throws(() => validateBinanceRequest(bad));
    assert.doesNotThrow(() => validateBinanceRequest({ method, headers, url: production }));
  }
});

test('Spot signature and raw-method oracle rejects each independent corruption', () => {
  for (const corrupt of [
    (m) => {m.params.signature = '0'.repeat(64);}, (m) => {m.params.apiKey = 'wrong';},
    (m) => {m.params.recvWindow = 60001;}, (m) => {m.params.timestamp -= 60000;},
    (m) => {m.params.timestamp += 1001;}, (m) => {m.params.unexpected = true;}, (m) => {m.method = 'userDataStream.subscribe';},
    (m) => {m.id = '';},
  ]) {
    const audit = createBinanceSpotSocketAudit('rust', {isPrivate:true});
    const m = subscribe(); corrupt(m); send(audit,m);
    assert.ok(audit.errors().length);
  }
  const audit = createBinanceSpotSocketAudit('rust',{isPrivate:true});
  send(audit,subscribe()); ack(audit);
  send(audit,{id:'heartbeat',method:'session.status'});
  assert.ok(audit.errors().some((e) => e.includes('Unexpected')));
});

test('Spot ACK correlation, subscription ID and reconnect generation remain exact', () => {
  const audit = createBinanceSpotSocketAudit('rust', {isPrivate:true});
  send(audit,subscribe()); assert.equal(audit.ready(),true); assert.equal(audit.ready(true),false); ack(audit); assert.equal(audit.ready(true),true);
  send(audit,{id:'heartbeat',method:'session.subscriptions'});
  receive(audit,{id:'heartbeat',status:200,result:[{subscriptionId:7}]});
  receive(audit,{subscriptionId:7,event:{e:'outboundAccountPosition',B:[]}});
  assert.deepEqual(audit.errors(),[]);
  for(const invalid of [
    {id:'wrong',status:200,result:{subscriptionId:7}},
    {subscriptionId:8,event:{e:'executionReport'}}, {e:'executionReport'},
  ]) receive(audit,invalid);
  audit.connected(URL,2);
  receive(audit,{subscriptionId:7,event:{e:'executionReport'}},2);
  assert.ok(audit.errors().some((e) => e.includes('Uncorrelated')));
  assert.ok(audit.errors().some((e) => e.includes('generation')));
});

test('Spot missing, duplicate and malformed ACKs cannot complete a valid replay', () => {
  for (const response of [undefined,{id:'subscribe',status:200,result:{subscriptionId:'7'}},{id:'subscribe',status:403,result:{} }]) {
    const audit=createBinanceSpotSocketAudit('rust',{isPrivate:true}); send(audit,subscribe());
    if(response) receive(audit,response);
    assert.ok(audit.errors().length);
  }
  const audit=createBinanceSpotSocketAudit('rust',{isPrivate:true}); send(audit,subscribe());ack(audit);ack(audit);
  assert.ok(audit.errors().some((e) => e.includes('Uncorrelated')));
});

test('Spot TS heartbeat reference checks the acknowledged own ID and keeps failures false', () => {
  const received=[];const sent=[];
  const feed={onMessage:(e)=>received.push(JSON.parse(e.data)),onClose(){},sendWithLog:(s)=>sent.push(JSON.parse(s))};
  prepareBinanceSpotPrivate({privateWebsocket:feed});feed.ping();
  assert.deepEqual(sent,[{id:42,method:'session.subscriptions'}]);
  const push=(m)=>feed.onMessage({data:JSON.stringify(m)});
  push({id:42,status:200,result:[{subscriptionId:7}]});assert.equal(received.at(-1).result.userDataStream,false);
  push({id:'tealstreet-user-data',status:200,result:{subscriptionId:7}});
  push({id:42,status:200,result:[{subscriptionId:7}]});assert.equal(received.at(-1).result.userDataStream,true);
  for (const result of [[],[{subscriptionId:8}],{userDataStream:true}]) {
    push({id:42,status:200,result});assert.equal(received.at(-1).result.userDataStream,false);
  }
  feed.onClose();push({id:42,status:200,result:[{subscriptionId:7}]});assert.equal(received.at(-1).result.userDataStream,false);
});

test('Spot TS public adapter uses independently seeded 1024-topic shards and production handlers', (t) => {
  t.mock.timers.enable({ apis: ['setTimeout'] });
  const sockets=[]; let tickerEvents; let bidEvents; let resets=0;
  class Socket { constructor(url){this.url=url;this.handlers={};this.sent=[];sockets.push(this);} addEventListener(type,fn){this.handlers[type]=fn;} send(text){this.sent.push(JSON.parse(text));} close(){this.closed=true;} }
  const feed={ws:{url:'wss://stream.binance.com:9443/ws'},resetOrderbook(){resets++;},subscribe(){},onMessage(){},
    handleTickerStreamEvents(events){tickerEvents=events;}, handleBookTickersStreamEvents(events){bidEvents=events;},dispose(){}};
  const exchange={publicWebsocket:feed,store:{markets:Array.from({length:2050},(_,i)=>({symbol:`TEST${i}`}))}};
  prepareBinanceSpotPublic(exchange,Socket);feed.onOpen();
  assert.equal(resets,1);assert.equal(sockets.length,3);
  for (const socket of sockets)socket.handlers.open();
  for (let part=0;part<8;part++) t.mock.timers.tick(400);
  assert.deepEqual(sockets.map((s)=>s.sent.flatMap((m)=>m.params).length),[1024,1024,2]);
  assert.ok(sockets.every((s)=>s.sent.every((m)=>m.params.length<=128)));
  assert.equal(sockets[2].sent[0].params[1],'test2049@ticker');
  feed.onOpen();assert.equal(sockets.length,3);
  sockets[1].handlers.close();t.mock.timers.tick(1000);assert.equal(sockets.length,4);
  sockets[3].handlers.open();assert.deepEqual(sockets[3].sent[0].params,sockets[1].sent[0].params);
  const events=[{s:'TEST0',b:'1',a:'2',c:'1.5'}];feed.handleTickerStreamEvents(events);
  assert.equal(tickerEvents,events);assert.equal(bidEvents,events);feed.dispose();assert.ok([sockets[0],sockets[2],sockets[3]].every((s)=>s.closed));
});

test('Spot private profile keeps wrong request method visible to its independent oracle', () => {
  const venue=venueOf('binancespot');
  const tape=new Tape([],venue,true,100,'rust');
  const socket={url:URL,ordinal:0,generation:1,requests:new Map(),requestIds:new Set(),requestCounts:new Map(),mappedRequests:new Map()};
  tape.audit.connected(URL,1);
  tape.send(socket,JSON.stringify({id:'wrong',method:'session.status'}));
  assert.ok(tape.audit.errors().some((e)=>e.includes('Unexpected')));
});


test('Spot fill correction uses only matching documented execution fields', () => {
  const records=[{type:'ws',text:JSON.stringify({subscriptionId:7,event:{e:'executionReport',x:'TRADE',c:'client',s:'BTCUSDT',l:'0.5',p:'100',L:'99'}})}];
  const account={fills:[{id:'client',symbol:'BTCUSDT',amount:0.5,price:100}]};
  fixBinanceSpotFills(account,records);assert.equal(account.fills[0].price,99);
  for (const field of ['id','symbol','amount']) {
    const bad={fills:[{id:'client',symbol:'BTCUSDT',amount:0.5,price:100,[field]:'wrong'}]};
    fixBinanceSpotFills(bad,records);assert.equal(bad.fills[0].price,100);
  }
});


test('Spot request correlation preserves numeric IDs independently of string spellings', () => {
  const audit=createBinanceSpotSocketAudit('ts',{isPrivate:true});send(audit,subscribe());ack(audit);
  send(audit,{id:42,method:'session.subscriptions'});
  receive(audit,{id:'42',status:200,result:{userDataStream:true}});
  assert.ok(audit.errors().some((e)=>e.includes('Uncorrelated')));
  const valid=createBinanceSpotSocketAudit('ts',{isPrivate:true});send(valid,subscribe());ack(valid);
  send(valid,{id:42,method:'session.subscriptions'});receive(valid,{id:42,status:200,result:[{subscriptionId:7}]});
  assert.deepEqual(valid.errors(),[]);
});

test('Binance timing rejects a correctly signed timestamp over one second in the future', () => {
  const timestamp=Date.now()+1500;
  const query=`symbol=BTCUSDT&timestamp=${timestamp}&recvWindow=20000`;
  assert.throws(()=>validateBinanceRequest({method:'GET',url:`https://api.binance.com/api/v3/openOrders?${query}&signature=${hmac(query)}`,headers:{'X-MBX-APIKEY':'replay-key'}}),/timestamp/);
  const audit=createBinanceSpotSocketAudit('rust',{isPrivate:true});
  const message=subscribe();message.params.timestamp=timestamp;
  message.params.signature=hmac(`apiKey=replay-key&recvWindow=5000&timestamp=${timestamp}`);
  send(audit,message);assert.ok(audit.errors().some((error)=>error.includes('signature/key/window')));
});


test('Binance wire identities omit only independently audited auth clocks/signatures and pure heartbeat', () => {
  const venue=venueOf('binancespot');const first=subscribe();const second=subscribe();
  second.params.timestamp+=1;second.params.signature='0'.repeat(64);
  assert.deepEqual(subscriptionKeys(URL,first,new Set(),venue),subscriptionKeys(URL,second,new Set(),venue));
  second.params.apiKey='wrong';assert.notDeepEqual(subscriptionKeys(URL,first,new Set(),venue),subscriptionKeys(URL,second,new Set(),venue));
  assert.deepEqual(subscriptionKeys(URL,{id:42,method:'LIST_SUBSCRIPTIONS'},new Set(),venue),[]);
  assert.notDeepEqual(subscriptionKeys(URL,{id:42,method:'LIST_SUBSCRIPTIONS',params:['unexpected']},new Set(),venue),[]);
});


test('Binance recorder ticker coverage cannot be satisfied by depth or subscription ACK frames', () => {
  assert.deepEqual(binanceTickerSymbols('{"e":"24hrTicker","s":"BTCUSDT"}'),['BTCUSDT']);
  assert.deepEqual(binanceTickerSymbols('[{"e":"24hrTicker","s":"ETHUSD_PERP"},{"e":"markPriceUpdate","s":"BTCUSD_PERP"}]'),['ETHUSD_PERP']);
  for (const text of ['{"result":null,"id":1}','{"e":"depthUpdate","s":"BTCUSDT"}'])assert.deepEqual(binanceTickerSymbols(text),[]);
});


test('filled-status TS correction is pinned to explicit native input identity and symbol', () => {
  for(const spot of [true,false]) {
    const native=spot?{e:'executionReport',X:'FILLED',s:'BTCUSDT',c:'original',i:123}:{e:'ORDER_TRADE_UPDATE',o:{X:'FILLED',s:'BTCUSD_PERP',i:123}};
    const symbol=spot?'BTCUSDT':'BTCUSD_PERP';const id=spot?'original':'123';
    const account={orders:[{symbol,id,status:'canceled'},{symbol,id:'other',status:'canceled'},{symbol:'OTHER',id,status:'canceled'}]};
    fixBinanceFilledStatus(account,[{type:'ws',text:JSON.stringify(spot?{subscriptionId:7,event:native}:native)}],spot);
    assert.deepEqual(account.orders.map((o)=>o.status),['closed','canceled','canceled']);
  }
});


test('Binance generated ACK IDs echo the matched signed query without weakening mutation identity', () => {
  const url='https://dapi.binance.com/dapi/v1/order';
  const venue={requestKey:venueOf('binanceinverse').requestKey};
  const tape=new Tape([{type:'http',method:'POST',url:url+'?symbol=BTCUSD_PERP&quantity=7',status:200,body:'{"orderId":123,"clientOrderId":"$newClientOrderId"}'}],venue);
  assert.equal(tape.respond('POST',url+'?symbol=BTCUSD_PERP&quantity=8&newClientOrderId=generated').status,599);
  assert.deepEqual(JSON.parse(tape.respond('POST',url+'?symbol=BTCUSD_PERP&quantity=7&newClientOrderId=generated').body),{orderId:123,clientOrderId:'generated'});
  assert.equal(tape.respond('POST',url+'?symbol=BTCUSD_PERP&quantity=7&newClientOrderId=generated').status,599);
});


test('Spot filled-status correction refuses reused client IDs naming a different live native order', () => {
  const records=[{e:'executionReport',X:'FILLED',s:'BTCUSDT',c:'reused',i:1},{e:'executionReport',X:'NEW',s:'BTCUSDT',c:'reused',i:2}]
    .map((event)=>({type:'ws',text:JSON.stringify({subscriptionId:7,event})}));
  const account={orders:[{symbol:'BTCUSDT',id:'reused',status:'open'}]};
  fixBinanceFilledStatus(account,records,true);assert.equal(account.orders[0].status,'open');
  const unambiguous={orders:[{symbol:'BTCUSDT',id:'reused',status:'canceled'}]};
  fixBinanceFilledStatus(unambiguous,[records[0]],true);assert.equal(unambiguous.orders[0].status,'closed');
});

test('Binance raw validation rejects foreign Spot/COIN-M aliases before generic identity normalization', () => {
  const signed = (path, fields, method = 'POST') => {
    const query = new URLSearchParams({ ...fields, timestamp: String(Date.now()), recvWindow: '5000' }).toString();
    const origin = path.startsWith('/dapi/') ? 'https://dapi.binance.com' : path.startsWith('/papi/') ? 'https://papi.binance.com' : path.startsWith('/fapi/') ? 'https://fapi.binance.com' : 'https://api.binance.com';
    return { method, url: `${origin}${path}?${query}&signature=${hmac(query)}`, headers: { 'X-MBX-APIKEY': 'replay-key' } };
  };
  for (const path of ['/api/v3/order', '/dapi/v1/order']) {
    for (const alias of ['c', 'clientOrderId', 'clientAlgoId', 'requestId']) {
      const request = signed(path, { symbol: path.startsWith('/dapi/') ? 'BTCUSD_PERP' : 'BTCUSDT', quantity: '7', [alias]: 'foreign' });
      assert.throws(() => validateBinanceRequest(request), /unsupported.*alias/i, `${path}: ${alias}`);
    }
  }
  for (const [path, fields, method] of [
    ['/api/v3/order', { symbol: 'BTCUSDT', newClientOrderId: 'native-placement' }],
    ['/api/v3/order', { symbol: 'BTCUSDT', origClientOrderId: 'native-cancel' }, 'DELETE'],
    ['/dapi/v1/order', { symbol: 'BTCUSD_PERP', newClientOrderId: 'native-placement' }],
    ['/dapi/v1/order', { symbol: 'BTCUSD_PERP', orderId: '123' }, 'DELETE'],
    ['/fapi/v1/algoOrder', { symbol: 'BTCUSDT', clientAlgoId: 'native-algo' }],
    ['/papi/v1/um/conditional/order', { symbol: 'BTCUSDT', newClientStrategyId: 'native-strategy' }],
  ]) assert.doesNotThrow(() => validateBinanceRequest(signed(path, fields, method)));
});

test('Spot currency oracle compares every native balance field without opaque info or value fallbacks', () => {
  const venue = venueOf('binancespot');
  assert.equal(venue.compareBalanceCurrencies, true);
  const currencies = { BTC: { free: 0.01, used: 0.001, total: 0.011, upnl: 0, info: { asset: 'BTC', locked: '0.001' } } };
  assert.deepEqual(venue.tsBalanceCurrencies(currencies), { BTC: { free: 0.01, used: 0.001, total: 0.011, upnl: 0 } });
  assert.deepEqual(venue.tsBalanceCurrencies({ BTC: { free: 0 } }), { BTC: { free: 0, used: undefined, total: undefined, upnl: undefined } });
  assert.deepEqual(currencies.BTC.info, { asset: 'BTC', locked: '0.001' });
});
