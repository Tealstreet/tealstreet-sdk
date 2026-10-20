import assert from 'node:assert/strict';
import test from 'node:test';
import { fixBlofinAccount, fixWeexConditionals, fixPhemexAccount } from './known-ts-fixes.mjs';

test('conditional oracle uses taped trigger and exposes an incorrect Rust price', () => {
  const account = { orders: [{ id: '702345678901234565', type: 'stop_market', reduceOnly: false, price: 0 }] };
  const records = [{ t: 1, type: 'ws', text: JSON.stringify({ e: 'orders', d: [{ id: account.orders[0].id, type: 'STOP_MARKET', triggerPrice: '71000', price: '0' }] }) }];
  fixWeexConditionals(account, records);
  assert.equal(account.orders[0].type, 'conditional');
  assert.equal(account.orders[0].price, 71000);
  assert.notDeepEqual(account, { orders: [{ ...account.orders[0], price: 1 }] });
});

test('REST seed and later stream update supply independent expected prices', () => {
  const account = { orders: [{ id: '1', type: 'conditional', price: 0 }] };
  fixWeexConditionals(account, [
    { t: 0, type: 'http', method: 'GET', url: 'https://example.test/openAlgoOrders', status: 200, body: '[{"algoId":"1","orderType":"STOP","triggerPrice":"70000"}]' },
    { t: 2, type: 'ws', text: '{"e":"orders","d":[{"id":"1","type":"STOP","triggerPrice":"71000"}]}' },
  ]);
  assert.equal(account.orders[0].price, 71000);
});

test('missing oracle and protective rows are not rewritten', () => {
  const account = { orders: [{ id: '1', type: 'stop_market', price: 0 }, { id: '2', type: 'stop_market', reduceOnly: true, price: 68000 }] };
  const before = structuredClone(account);
  fixWeexConditionals(account, [{ t: 1, type: 'http', method: 'POST', url: 'https://example.test/algoOrder', status: 200, requestBody: '{"type":"STOP_MARKET","triggerPrice":"68000","reduceOnly":true}', body: '{"orderId":"2"}' }]);
  assert.deepEqual(account, before);
});

test('all native protective markers prevent conditional rewriting', () => {
  for (const marker of [{ algoType: 'TPSL' }, { algoType: 'TP_SL' }, { positionTpsl: true }]) {
    const account = { orders: [{ id: '1', type: 'stop_market', price: 68000 }] };
    fixWeexConditionals(account, [{ t: 1, type: 'ws', text: JSON.stringify({ e: 'orders', d: [{ id: '1', type: 'STOP_MARKET', triggerPrice: '71000', ...marker }] }) }]);
    assert.equal(account.orders[0].type, 'stop_market');
    assert.equal(account.orders[0].price, 68000);
  }
});

test('numeric 18-digit identifiers retain their exact recorded spelling', () => {
  const account = { orders: [{ id: '702345678901234565', type: 'conditional', price: 0 }] };
  fixWeexConditionals(account, [{ t: 1, type: 'ws', text: '{"e":"orders","d":[{"id":702345678901234565,"type":"STOP_MARKET","triggerPrice":"71000"}]}' }]);
  assert.equal(account.orders[0].price, 71000);
});


test('BloFin unavailable margin cannot be defined by a wrong positive Rust margin', () => {
  const account = { positions: [{ symbol: 'BTCUSDT', side: 'long', marginSize: 0,
    contracts: 0.004, entryPrice: 59965.8, notional: 240, leverage: 30 }], balance: { used: 10, upnl: 2 } };
  const wrongRust = structuredClone(account);
  wrongRust.positions[0].marginSize = 123456;
  fixBlofinAccount(account, wrongRust);
  assert.equal(account.positions[0].marginSize, null);
  assert.notDeepEqual(account, wrongRust);
  assert.equal(wrongRust.positions[0].marginSize, 123456);
  // A plausible notional/leverage value is also unsupported per-position margin.
  wrongRust.positions[0].marginSize = 8;
  assert.notDeepEqual(account, wrongRust);
});

test('BloFin only repairs placeholder zero and preserves explicit TS margin values', () => {
  const account = { positions: [{ marginSize: 0 }, { marginSize: null }, { marginSize: 25 }], balance: { used: 10, upnl: 2 } };
  fixBlofinAccount(account);
  assert.deepEqual(account.positions, [{ marginSize: null }, { marginSize: null }, { marginSize: 25 }]);
  assert.deepEqual(account.balance, { used: 10, upnl: 2 });
  account.positions = [];
  fixBlofinAccount(account);
  assert.deepEqual(account.balance, { used: 0, upnl: 0 });
});

test('Phemex oracle derives unique fills, cumulative exposure and flat settings from tape only', () => {
  const account={ orders:[{ id:'order', filled:0, remaining:1 }], fills:[], settings:[] };
  const fill={ orderID:'order', symbol:'BTCUSDT', side:'Buy', execStatus:'MakerFill', execID:'one', execQty:'0.1', execPriceRp:'10', orderQty:'1', cumQty:'0.1' };
  const records=[{ type:'http', status:200, url:'https://api.phemex.com/g-accounts/accountPositions', body:JSON.stringify({ data:{ positions:[{symbol:'ETHUSDC',size:'0',leverageRr:'-20'}] } }) }, { type:'ws', text:JSON.stringify({orders_p:[fill,{...fill,execID:'two',execQty:'0.2',cumQty:'0.3'}]}) }, { type:'ws', text:JSON.stringify({orders_p:[{...fill,execID:'two',execQty:'0.2',cumQty:'0.3'}]}) }];
  fixPhemexAccount(account,records);
  assert.deepEqual(account.fills.map((f)=>f.amount),[0.1,0.2]);
  assert.deepEqual(account.orders,[{id:'order',filled:0.3,remaining:0.7}]);
  assert.deepEqual(account.settings,[{symbol:'ETHUSDC',leverage:20,marginType:'cross'}]);
  assert.notEqual(account.fills[1].amount,0.1);
});

test('Phemex flat settings use the latest row and reject stale row time behind a newer envelope', () => {
  const account={orders:[],fills:[],settings:[]};
  const row=(leverage,time)=>({symbol:'ETHUSDC',posSide:'Long',size:'0',leverageRr:leverage,transactTimeNs:time});
  const records=[{t:0,type:'http',status:200,url:'https://api.phemex.com/g-accounts/accountPositions',body:JSON.stringify({data:{positions:[row('-10','10')]}})}, {t:1,type:'ws',text:JSON.stringify({timestamp:'20',positions_p:[row('20','20')]})}, {t:2,type:'ws',text:JSON.stringify({timestamp:'30',positions_p:[row('-10','10')]})}];
  fixPhemexAccount(account,records);
  assert.deepEqual(account.settings,[{symbol:'ETHUSDC',leverage:20,marginType:'isolated'}]);
});
