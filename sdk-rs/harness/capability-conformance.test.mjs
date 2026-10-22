import assert from 'node:assert/strict';
import { test } from 'node:test';
import { advertisedCommands, checkAdvertisedCommands, probeCapabilityCommand, requireCapabilityEngine, constructionProfiles } from './capability-conformance.mjs';

const command = { op: 'fetchOrderBook', command: 17, symbol: 'BTC/USDT' };
const engine = (actions) => ({ command: () => JSON.stringify(actions) });

test('same-family construction errors cannot silently remove coverage', () => {
  assert.throws(() => requireCapabilityEngine(() => { throw new Error('testnet required'); }, 'txflow'), /txflow/);
  assert.throws(() => requireCapabilityEngine(() => undefined, 'missing instance'), /missing instance/);
});

test('a returned unsupported command failure cannot pass as decoder success', () => {
  assert.throws(() => probeCapabilityCommand(engine([
    { op: 'done', command: 17, error: 'not implemented', detail: { kind: 'unsupported' } },
  ]), command, 'advertised book'), /advertised book/);
});

test('a coded unsupported event cannot pass as decoder success', () => {
  assert.throws(() => probeCapabilityCommand(engine([
    { op: 'event', level: 'error', code: 'command.unsupported', message: 'unsupported command' },
  ]), command, 'advertised book'), /advertised book/);
});


// Schema fixture only: expectations are selected from the runtime record, never
// a table saying which production venue should advertise which capability.
function record(extra = {}) {
  return {
    account: 'unsupported', clientOrderLookup: false,
    fillHistory: false, orderHistory: false, incomeHistory: false, fillsByOrderId: false,
    cancelAll: 'eachOpenSymbol', leverageQuery: 'settings', maxLeverage: 'market',
    accountLeverage: false, accountMarginType: false, cancelWithSymbols: false,
    amendWithSymbol: false, orderBookQuery: false, options: false, quantityUnits: 'base',
    accountTier: false, closedPositions: false, balanceInfo: false, marketRefresh: false,
    positionHistory: 'unsupported', closedPositionFormat: 'unsupported', fillHistoryTime: 'asProvided',
    stpControls: false, bybitExecutionControls: false, rejectClientSlippage: false,
    finiteCommandNumbers: false, finiteHistoryBounds: false, rejectOrderHistoryFilters: false,
    publicCatalogueValidation: false, isolatedBookSubscriptions: false,
    inverseClosedPositions: false, scheduleCancel: false, ...extra,
  };
}

test('only runtime-advertised commands are selected; policy fields add no ops', () => {
  assert.deepEqual(advertisedCommands(record()), []);
  const probes = advertisedCommands(record({
    orderBookQuery: true, scheduleCancel: true, options: true,
    bybitExecutionControls: true, quantityUnits: 'contractCount',
  }));
  assert.deepEqual(probes.map((probe) => probe.command.op), [
    'fetchOrderBook', 'fetchOptionChain', 'fetchOptionExpiries', 'fetchAccountGreeks',
    'scheduleCancel', 'scheduleCancel',
  ]);
  assert.equal(new Set(probes.map((probe) => probe.command.command)).size, probes.length);
});

test('unknown, missing, mistyped and unknown-enum capabilities fail closed', () => {
  for (const bad of [record({ futureFlag: false }), record({ options: 1 }),
    record({ account: 'maybe' }), record({ maxLeverage: null })]) {
    assert.throws(() => advertisedCommands(bad));
  }
  const missing = record();
  delete missing.orderBookQuery;
  assert.throws(() => advertisedCommands(missing));
});

test('missing or malformed action output is not evidence', () => {
  for (const output of [undefined, '{', '{}', '[null]', '[{}]', '[{"op":"done","command":17}]', '[{"op":"futureAction"}]']) {
    assert.throws(() => probeCapabilityCommand({ command: () => output }, command, 'invalid output'), /invalid output/);
  }
});

test('completion ownership is checked instead of accepting a neighbour result', () => {
  assert.throws(() => probeCapabilityCommand(engine([
    { op: 'done', command: 18, result: '[]' },
  ]), command, 'wrong owner'), /wrong owner/);
});

test('preconditions, terminal results, requests and deferred output remain distinct', () => {
  const cases = [
    [[{ op: 'done', command: 17, error: 'Unknown market', detail: { kind: 'invalidInput' } }], 'precondition'],
    [[{ op: 'done', command: 17, result: '[]' }], 'terminal'],
    [[{ op: 'http', request: 1, method: 'GET', url: 'https://example.test', headers: [] }], 'request'],
    [[], 'deferred'],
  ];
  for (const [actions, evidence] of cases) {
    assert.equal(probeCapabilityCommand(engine(actions), command, evidence).evidence, evidence);
  }
  // None of these is labelled positive valid-state handler proof.
});

test('each probe gets a fresh instance and all instances are freed', () => {
  let next = 0;
  const freed = [];
  const calls = [];
  const proofs = checkAdvertisedCommands(() => {
    const id = next++;
    return {
      capabilities: () => JSON.stringify(record({ scheduleCancel: true })),
      command: (json) => { calls.push(id); return JSON.stringify([{ op: 'done', command: JSON.parse(json).command, error: 'Signing key missing' }]); },
      free: () => freed.push(id),
    };
  }, 'isolated');
  assert.deepEqual(calls, [1, 2]);
  assert.deepEqual(freed, [0, 1, 2]);
  assert.deepEqual(proofs.map((proof) => proof.evidence), ['precondition', 'precondition']);
});

test('decoder rejection and unsupported dispatch never become deferred evidence', () => {
  assert.throws(() => probeCapabilityCommand({ command() { throw new Error('unknown variant fetchOrderBook'); } }, command, 'MEXC missing decoder'), /MEXC missing decoder/);
  let freed = 0;
  assert.throws(() => checkAdvertisedCommands(() => ({
    capabilities: () => JSON.stringify(record({ orderBookQuery: true })),
    command: () => JSON.stringify([{ op: 'done', command: 1, error: 'unsupported' }]),
    free: () => freed++,
  }), 'lying capability'), /lying capability/);
  assert.equal(freed, 2);
});


test('construction fixture covers known prerequisites and the options profile', () => {
  assert.equal(constructionProfiles('txflow')[0].testnet, true);
  assert.equal(constructionProfiles('breakoutprop')[0].key, 'user');
  assert.equal(constructionProfiles('hyrotrader')[0].endpoints.rest, 'https://hyro.test');
  assert.deepEqual(constructionProfiles('bybit').map((options) => options.enableOptionMarkets ?? false), [false, true]);
});


test('request evidence requires real wire fields and terminal evidence is unique and typed', () => {
  for (const actions of [
    [{ op: 'http' }], [{ op: 'sign' }], [{ op: 'signPayload' }], [{ op: 'wsSend' }],
    [{ op: 'event' }],
    [{ op: 'done', command: 17, result: '[]', error: 123 }],
    [{ op: 'done', command: 17, result: '[]' }, { op: 'done', command: 17, result: '[]' }],
  ]) assert.throws(() => probeCapabilityCommand(engine(actions), command, 'malformed proof'), /malformed proof/);
});

test('an unsupported input symbol is ingress-only, not an unavailable command', () => {
  const proof = probeCapabilityCommand(engine([
    { op: 'done', command: 17, error: 'Unsupported native history symbol' },
  ]), command, 'symbol precondition');
  assert.equal(proof.evidence, 'precondition');
});
