import assert from 'node:assert/strict';
import { test } from 'node:test';
import { auditFixerChanges, FIXER_CONTRACTS } from './fixer-contracts.mjs';
import { runAuditedFixer } from './oracle-audit.mjs';
import { createTsFixes } from './ts-oracle.mjs';

const reference = (commands) => ({ markets: [], tickers: [], books: {}, requests: [], subscriptions: [], commands });
const empty = () => ({ ok: [] });

test('KuCoin query corrections retain positional command identity across repeated empty results', () => {
  const expected = [{ id: 'fill', price: 89.5, qty: 0.001, commission: 0.0001 }];
  const records = [
    { type: 'command', op: 'fetchFills', expected },
    { type: 'command', op: 'cancelOrders' },
    { type: 'command', op: 'placeOrders' },
    { type: 'command', op: 'cancelOrders' },
  ];
  const output = reference([empty(), empty(), { ok: ['fixed-command-result'] }, empty()]);
  const before = structuredClone(output);
  const audit = runAuditedFixer('kucoin', output, createTsFixes({ venue: 'kucoin' }, records).applyTsCorrections);
  assert.deepEqual(audit.errors, []);
  assert.deepEqual(output.commands, [{ ok: expected }, ...before.commands.slice(1)]);
  assert.equal(audit.counts['commands@order'], undefined);
  assert.deepEqual(auditFixerChanges('kucoin', before, output).errors, []);
});

test('repeated equal rows do not hide genuine permutations or grant command reordering', () => {
  const a = { id: 'a', price: 1 }, b = { id: 'b', price: 2 }, c = { id: 'c', price: 3 };
  for (const [before, after] of [
    [[a, a, b], [a, b, a]],
    [[a, b, c], [c, a]],
    [[a, b], [b, a]],
  ]) assert.ok(auditFixerChanges('kucoin', { commands: [{ ok: before }] }, { commands: [{ ok: after }] })
    .errors.some((error) => error.includes('commands[0].ok@order')));
  const before = reference([{ ok: [{ id: 'a' }] }, { ok: [{ id: 'b' }] }]);
  const after = structuredClone(before); after.commands.reverse();
  assert.ok(auditFixerChanges('kucoin', before, after).errors.some((error) => error.includes('commands@order')));
});

const orderlyVenues = ['orderly', 'tealstreet-dex', 'kodiakfi', 'mode', 'salsadex', 'what', 'woofi'];
const leverageRecords = () => [
  { type: 'meta', private: true },
  { type: 'command', op: 'fetchLeverage', symbol: 'BTCUSDC' },
  { type: 'http', method: 'GET', url: 'https://example.test/v1/client/leverage?symbol=PERP_BTC_USDC&margin_mode=CROSS',
    status: 200, body: JSON.stringify({ success: true, data: { max_leverage: 12, margin_mode: 'CROSS' } }) },
];

test('Orderly symbol-leverage fallback initializes only its documented positive pair', () => {
  for (const venue of orderlyVenues) {
    const output = reference([{ ok: null }]);
    const audit = runAuditedFixer(venue, output, createTsFixes({ venue }, leverageRecords()).applyTsCorrections);
    assert.deepEqual(audit.errors, [], venue);
    assert.deepEqual(output.commands, [{ ok: { buyLeverage: 12, sellLeverage: 12 } }], venue);
    assert.equal(audit.counts['commands[0].ok@type'], 1, venue);
    assert.ok(Object.isFrozen(FIXER_CONTRACTS[venue].nullObjects));
    assert.ok(Object.isFrozen(FIXER_CONTRACTS[venue].nullObjects[0].fields));
  }
});

test('Orderly leverage shape is not a generic response initialization permission', () => {
  for (const result of [[], {}, 12, '12', { buyLeverage: 12 }, { buyLeverage: 12, sellLeverage: 12, extra: 1 },
    { buyLeverage: 0, sellLeverage: 12 }, { buyLeverage: -1, sellLeverage: 12 },
    { buyLeverage: NaN, sellLeverage: 12 }, { buyLeverage: Infinity, sellLeverage: 12 },
    { buyLeverage: '12', sellLeverage: 12 }, { buyLeverage: [], sellLeverage: 12 },
    { id: 'arbitrary-query-row' }]) {
    const audit = auditFixerChanges('orderly', reference([{ ok: null }]), reference([{ ok: result }]));
    assert.ok(audit.errors.length, JSON.stringify(result));
  }
  const pair = { buyLeverage: 12, sellLeverage: 12 };
  assert.ok(auditFixerChanges('kucoin', reference([{ ok: null }]), reference([{ ok: pair }])).errors.length);
  assert.ok(auditFixerChanges('orderly', reference([{ ok: 1 }]), reference([{ ok: pair }])).errors.length);
  assert.ok(auditFixerChanges('orderly', reference([{ ok: pair }]), reference([{ ok: null }])).errors.length);
});

test('already implemented or unexpected Orderly leverage results are never overwritten', () => {
  const pair = { buyLeverage: 12, sellLeverage: 12 };
  const output = reference([{ ok: pair }]);
  const audit = runAuditedFixer('orderly', output, createTsFixes({ venue: 'orderly' }, leverageRecords()).applyTsCorrections);
  assert.deepEqual(audit.errors, []);
  assert.equal(audit.total, 0);
  assert.deepEqual(output.requests, []);
  const unexpected = reference([{ ok: { buyLeverage: 13, sellLeverage: 13 } }]);
  assert.throws(() => createTsFixes({ venue: 'orderly' }, leverageRecords()).applyTsCorrections(unexpected), /unexpected TS symbol leverage fallback/);
});

test('terminal Orderly row removal includes both leg triggers without permitting retained-row rewrites', () => {
  const order = { id: 'terminal', type: 'limit', takeProfitTrigger: 'mark_price', stopLossTrigger: 'last_price' };
  const before = { account: { orders: [order] } };
  assert.deepEqual(auditFixerChanges('orderly', before, { account: { orders: [] } }).errors, []);
  for (const field of ['takeProfitTrigger', 'stopLossTrigger']) {
    const after = structuredClone(before); after.account.orders[0][field] = 'index_price';
    assert.ok(auditFixerChanges('orderly', before, after).errors.some((error) => error.includes(`.${field}`)));
  }
});
