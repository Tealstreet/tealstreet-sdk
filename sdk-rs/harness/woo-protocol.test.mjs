import test from 'node:test';
import assert from 'node:assert/strict';
import { createHmac } from 'node:crypto';
import { createWooSocketAudit, validateWooRequest } from './woo-protocol.mjs';
const sign = (s) => createHmac('sha256','replay-secret').update(s).digest('hex');
test('WOO signs V1 form bytes and V3 JSON bytes without rewriting the body', () => {
  const time = Date.now();
  for (const [url, body, text, type] of [
    ['/v1/order','client_order_id=9007199254740991&symbol=PERP_BTC_USDT',`client_order_id=9007199254740991&symbol=PERP_BTC_USDT|${time}`,'application/x-www-form-urlencoded'],
    ['/v3/order/12','{"quantity":"1"}',`${time}PUT/v3/order/12{"quantity":"1"}`,'application/json'],
  ]) {
    const request = { method:'PUT',url:`https://api.woox.io${url}`,body,headers:{'x-api-key':'replay-key','x-api-timestamp':time,'x-api-signature':sign(text),'content-type':type} };
    validateWooRequest(request);
    assert.throws(() => validateWooRequest({...request,body:body+' '}),/signature/);
    assert.throws(() => validateWooRequest({...request,headers:{...request.headers,'x-api-signature':'bad'}}),/signature/);
  }
});
test('WOO authentication, native subscriptions and reconnect are audited', () => {
  const audit=createWooSocketAudit('rust',{isPrivate:true,books:['BTCUSDT']});
  const pub='wss://wss.woox.io/ws/stream/undefined',priv='wss://wss.woox.io/v2/ws/private/stream/undefined';
  audit.connected(pub);audit.connected(priv);
  for(const topic of ['tickers','bbos','markprices','PERP_BTC_USDT@orderbookupdate'])audit.sent(pub,JSON.stringify({event:'subscribe',topic}));
  const timestamp=Date.now();audit.sent(priv,JSON.stringify({event:'auth',params:{apikey:'replay-key',timestamp,sign:sign(`|${timestamp}`)}}));assert.equal(audit.ready(),true);
  audit.received(priv,JSON.stringify({event:'auth',success:true}));
  for(const topic of ['executionreport','algoexecutionreportv2'])audit.sent(priv,JSON.stringify({event:'subscribe',topic}));
  for(const url of [pub,priv])audit.sent(url,'{"event":"ping"}');
  assert.deepEqual(audit.errors(),[]);
  audit.connected(priv);audit.received(priv,'{"topic":"executionreport","data":{}}');assert.ok(audit.errors().length>0);
});
test('a valid V1 signature cannot hide unsorted form or query parameters', () => {
  const timestamp = Date.now();
  const form = 'symbol=PERP_BTC_USDT&client_order_id=123';
  for (const method of ['GET', 'POST']) {
    const request = { method, url: `https://api.woox.io/v1/order${method === 'GET' ? `?${form}` : ''}`, body: method === 'GET' ? undefined : form, headers: {
      'x-api-key': 'replay-key', 'x-api-timestamp': timestamp,
      'x-api-signature': sign(`${form}|${timestamp}`), 'content-type': 'application/x-www-form-urlencoded',
    } };
    assert.throws(() => validateWooRequest(request), /not sorted/);
  }
});
test('each WOO server ping requires a pong on the same socket', () => {
  const url = 'wss://wss.woox.io/ws/stream/undefined';
  const audit = createWooSocketAudit('rust', { isPrivate: false });
  audit.connected(url);
  audit.sent(url, '{"event":"ping"}');
  audit.received(url, '{"event":"ping"}');
  audit.sent(url, '{"event":"pong"}');
  assert.deepEqual(audit.errors(), []);
  const missing = createWooSocketAudit('ts', { isPrivate: false });
  missing.connected(url);
  missing.sent(url, '{"event":"ping"}');
  missing.received(url, '{"event":"ping"}');
  assert.ok(missing.errors().some((error) => error.includes('server ping was not answered')));
});
