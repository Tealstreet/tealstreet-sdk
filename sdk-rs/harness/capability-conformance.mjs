// One capability-to-command contract for combined and isolated shipped artifacts.
// Requests returned by raw WASM are inspected, never sent to a network.
import assert from 'node:assert/strict';

const symbol = '__capability_probe__';
const enums = {
  account: ['unsupported', 'query', 'afterLoad'],
  cancelAll: ['eachOpenSymbol', 'eachPlacedSymbol', 'native'],
  leverageQuery: ['settings', 'onRead', 'onMetadata', 'onReadAndMetadata'],
  maxLeverage: ['market', 'query', 'rails', 'unsupported'],
  quantityUnits: ['base', 'nativePlacement', 'inverseNative', 'contractCount', 'inverseContractSized'],
  positionHistory: ['unsupported', 'fillsAccount', 'fillsSymbol', 'bybit', 'okx', 'blofin', 'wooOrders'],
  closedPositionFormat: ['unsupported', 'bybit', 'okx', 'blofin'],
  fillHistoryTime: ['asProvided', 'secondsOrMilliseconds', 'nonnegativeSecondsOrMilliseconds'],
};
// These fields affect arguments, frames or bridge policy, not a new command op.
const policies = new Set([
  'quantityUnits', 'positionHistory', 'closedPositionFormat', 'fillHistoryTime',
  'bybitExecutionControls', 'rejectClientSlippage', 'finiteCommandNumbers',
  'finiteHistoryBounds', 'rejectOrderHistoryFilters', 'publicCatalogueValidation',
  'isolatedBookSubscriptions', 'inverseClosedPositions',
]);
const conditional = {
  account: (v) => v === 'query' ? [{ op: 'fetchAccount' }] : [],
  cancelAll: (v) => v === 'native' ? [{ op: 'cancelSymbolOrders', symbol: '' }] : [],
  leverageQuery: (v) => v !== 'settings' ? [{ op: 'fetchLeverage', symbol }] : [],
  maxLeverage: (v) => v === 'query' ? [{ op: 'fetchMaxLeverage', symbol }] : [],
  clientOrderLookup: () => [{ op: 'findOrderByClientOrderId', symbol, clientOrderId: 'probe' }],
  fillHistory: () => [{ op: 'fetchFills', symbol, limit: 1 }],
  orderHistory: () => [{ op: 'fetchOrderHistory', symbol, limit: 1 }],
  incomeHistory: () => [{ op: 'fetchIncome', symbol, limit: 1 }],
  fillsByOrderId: () => [{ op: 'fetchFills', symbol, orderId: 'probe', limit: 1 }],
  accountLeverage: () => [{ op: 'setLeverage', symbol: '', leverage: 1 }],
  accountMarginType: () => [{ op: 'setMarginType', symbol: '', isolated: false }],
  cancelWithSymbols: () => [{ op: 'cancelOrdersWithSymbols', orders: [{ id: 'probe', symbol }] }],
  amendWithSymbol: () => [{ op: 'amendOrderWithSymbol', id: 'probe', symbol, price: 1 }],
  orderBookQuery: () => [{ op: 'fetchOrderBook', symbol, limit: 1 }],
  options: () => [
    { op: 'fetchOptionChain', query: { underlying: 'BTC' } },
    { op: 'fetchOptionExpiries', underlying: 'BTC' },
    { op: 'fetchAccountGreeks' },
  ],
  accountTier: () => [{ op: 'fetchAccountTier' }, { op: 'setAccountTier', tier: 'standard' }],
  closedPositions: () => [{ op: 'fetchClosedPositions', symbol }],
  balanceInfo: () => [{ op: 'fetchBalanceInfo' }],
  marketRefresh: () => [{ op: 'refreshMarkets' }],
  stpControls: () => [{ op: 'fetchStpMode' }, { op: 'setStpMode', mode: 'EXPIRE_TAKER' }],
  scheduleCancel: () => [{ op: 'scheduleCancel', time: 5000 }, { op: 'scheduleCancel', time: null }],
};
const fields = [...new Set([...Object.keys(enums), ...Object.keys(conditional), ...policies])];

export function advertisedCommands(record) {
  assert.ok(record && typeof record === 'object' && !Array.isArray(record), 'capability record must be an object');
  assert.deepEqual(Object.keys(record).sort(), [...fields].sort(), 'every capability field must be classified');
  const probes = [];
  for (const field of fields) {
    const value = record[field];
    if (enums[field]) assert.ok(enums[field].includes(value), `${field}: unknown enum value ${value}`);
    else assert.equal(typeof value, 'boolean', `${field}: expected boolean`);
    if (!policies.has(field) && conditional[field] && value !== false) {
      for (const command of conditional[field](value)) {
        probes.push({ field, command: { ...command, command: probes.length + 1 } });
      }
    }
  }
  return probes;
}

export function requireCapabilityEngine(create, label) {
  let instance;
  try { instance = create(); } catch (error) {
    assert.fail(`${label}: construction failed: ${error?.message ?? error}`);
  }
  assert.ok(instance && typeof instance.command === 'function'
    && typeof instance.capabilities === 'function' && typeof instance.free === 'function',
  `${label}: missing usable engine`);
  return instance;
}

export function probeCapabilityCommand(instance, command, label) {
  let output;
  assert.doesNotThrow(() => { output = instance.command(JSON.stringify(command), 0); },
    `${label}: advertised command rejected by compiled ingress`);
  assert.equal(typeof output, 'string', `${label}: missing encoded actions`);
  let actions;
  assert.doesNotThrow(() => { actions = output === '' ? [] : JSON.parse(output); }, `${label}: malformed encoded actions`);
  assert.ok(Array.isArray(actions), `${label}: actions must be an array`);
  const knownOps = new Set(['accountAuth', 'deactivate', 'startupFailed', 'wsConnect', 'wsSend', 'wsClose', 'http', 'sign', 'signPayload', 'setTimer', 'clearTimer', 'event', 'commandError', 'done', 'commandProgress']);
  for (const action of actions) {
    assert.ok(action && typeof action === 'object' && typeof action.op === 'string', `${label}: malformed action`);
    assert.ok(knownOps.has(action.op), `${label}: unknown action op ${action.op}`);
    if (action.op === 'done') {
      assert.ok((typeof action.result === 'string') !== (typeof action.error === 'string'), `${label}: malformed terminal output`);
    }
    assert.notEqual(action.detail?.kind, 'unsupported', `${label}: advertised handler is unsupported`);
    assert.notEqual(action.code, 'command.unsupported', `${label}: advertised handler is unsupported`);
    if (action.op === 'done' || action.op === 'commandError') {
      assert.equal(action.command, command.command, `${label}: uncorrelated command output`);
      assert.doesNotMatch(action.error ?? action.message ?? '', /unsupported|not supported|not implemented/i,
        `${label}: advertised handler is unsupported`);
    }
  }
  // Fresh instance per probe: no startup/earlier command can supply these actions.
  // This result is ingress evidence, not valid-state positive handler proof.
  const terminal = actions.find((action) => action.op === 'done');
  const evidence = terminal ? (terminal.error ? 'precondition' : 'terminal')
    : actions.some((action) => ['http', 'sign', 'signPayload', 'wsSend'].includes(action.op)) ? 'request' : 'deferred';
  return { evidence, actions };
}


export function checkAdvertisedCommands(create, label) {
  const initial = requireCapabilityEngine(create, label);
  let record;
  let probes;
  try {
    record = JSON.parse(initial.capabilities());
    probes = advertisedCommands(record);
  } finally { initial.free(); }
  return probes.map(({ field, command }) => {
    const instance = requireCapabilityEngine(create, `${label} ${field}/${command.op}`);
    try {
      assert.deepEqual(JSON.parse(instance.capabilities()), record, `${label}: unstable capability record`);
      return { field, op: command.op, ...probeCapabilityCommand(instance, command, `${label} ${field}/${command.op}`) };
    } finally { instance.free(); }
  });
}


// Construction data only; this does not say which capabilities any venue has.
export function constructionProfiles(venue) {
  const base = { publicDataOnly: true, testnet: true };
  if (venue === 'breakoutprop') Object.assign(base, { key: 'user', secret: 'password' });
  if (venue === 'hyrotrader') base.endpoints = {
    rest: 'https://hyro.test', publicWs: 'wss://hyro.test/public', privateWs: 'wss://hyro.test/private',
  };
  return venue === 'bybit' ? [base, { ...base, enableOptionMarkets: true }] : [base];
}
