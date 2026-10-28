// One capability-to-command contract for combined and isolated shipped artifacts.
// Requests returned by raw WASM are inspected, never sent to a network.
import assert from 'node:assert/strict';
import { randomBytes } from 'node:crypto';

const symbol = '__capability_probe__';
const enums = {
  account: ['unsupported', 'query', 'afterLoad'],
  cancelAll: ['eachOpenSymbol', 'eachPlacedSymbol', 'native'],
  leverageQuery: ['settings', 'onRead', 'onMetadata', 'onReadAndMetadata'],
  maxLeverage: ['market', 'query', 'rails', 'unsupported'],
  quantityUnits: ['base', 'nativePlacement', 'inverseNative', 'contractCount', 'inverseContractSized'],
  positionHistory: ['unsupported', 'fillsAccount', 'fillsSymbol', 'bybit', 'okx', 'blofin', 'wooOrders', 'bitunix'],
  closedPositionFormat: ['unsupported', 'bybit', 'okx', 'blofin', 'bitunix'],
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
  account: (v) => v !== 'unsupported' ? [{ op: 'fetchAccount' }] : [],
  cancelAll: (v) => [{ op: 'cancelSymbolOrders', symbol: v === 'native' ? '' : symbol }],
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

const object = (value) => value !== null && typeof value === 'object' && !Array.isArray(value);
const u32 = (value) => Number.isInteger(value) && value >= 0 && value <= 0xffff_ffff;
const headers = (value) => Array.isArray(value) && value.every((pair) => Array.isArray(pair)
  && pair.length === 2 && pair.every((item) => typeof item === 'string'));
function closed(value, required, optional = []) {
  return object(value) && required.every((key) => Object.hasOwn(value, key))
    && Object.keys(value).every((key) => required.includes(key) || optional.includes(key));
}
function token(value, prefix, count) {
  if (typeof value !== 'string' || value.length > 80) return;
  const words = value.split(':');
  if (words.shift() !== prefix || words.length !== count
    || words.some((word) => !/^(?:0|[1-9][0-9]*)$/.test(word) || word.length > 20)) return;
  const values = words.map((word) => BigInt(word));
  if (values.some((part) => part > 0xffff_ffff_ffff_ffffn)
    || values[0] === 0n || values[0] > 0xffff_ffffn || values[1] === 0n
    || (prefix === 'r1' && values[3] === 0n)) return;
  return words;
}
function managedHttp(action) {
  const preparation = token(action.preparation, 'p1', 3);
  if (!preparation) return false;
  if (action.phase === 'cancel') {
    return closed(action, ['op', 'phase', 'preparation'], ['signing'])
      && (!Object.hasOwn(action, 'signing') || u32(action.signing));
  }
  if (action.phase === 'prepare') {
    if (!closed(action, ['op', 'phase', 'preparation', 'intent', 'packet'])
      || !['public', 'primary', 'secondary'].includes(action.intent)) return false;
  } else if (action.phase === 'bind') {
    if (!closed(action, ['op', 'phase', 'preparation', 'permit', 'packet'])) return false;
    const permit = token(action.permit, 'r1', 4);
    // Resource id and preparation sequence are different counters. Only the
    // physical request and attempt generation must match across these tokens.
    if (!permit || permit[0] !== preparation[0] || permit[1] !== preparation[1]) return false;
  } else return false;
  const packet = action.packet;
  return closed(packet, ['request', 'method', 'url', 'headers'], ['route', 'timeoutMs', 'body'])
    && u32(packet.request) && String(packet.request) === preparation[0]
    && ['GET', 'POST', 'PUT', 'DELETE', 'PATCH'].includes(packet.method)
    && typeof packet.url === 'string' && headers(packet.headers)
    && (!Object.hasOwn(packet, 'route') || ['primary', 'direct', 'publicCors'].includes(packet.route))
    && (!Object.hasOwn(packet, 'timeoutMs') || u32(packet.timeoutMs))
    && (!Object.hasOwn(packet, 'body') || typeof packet.body === 'string');
}

export function probeCapabilityCommand(instance, command, label) {
  let output;
  assert.doesNotThrow(() => { output = instance.command(JSON.stringify(command), 0); },
    `${label}: advertised command rejected by compiled ingress`);
  assert.equal(typeof output, 'string', `${label}: missing encoded actions`);
  let actions;
  assert.doesNotThrow(() => { actions = output === '' ? [] : JSON.parse(output); }, `${label}: malformed encoded actions`);
  assert.ok(Array.isArray(actions), `${label}: actions must be an array`);
  let terminalCount = 0;
  for (const action of actions) {
    assert.ok(action && typeof action === 'object' && !Array.isArray(action) && typeof action.op === 'string', `${label}: malformed action`);
    const integer = (value) => Number.isSafeInteger(value) && value >= 0;
    const object = (value) => value !== null && typeof value === 'object' && !Array.isArray(value);
    const headers = (value) => Array.isArray(value) && value.every((pair) => Array.isArray(pair) && pair.length === 2 && pair.every((item) => typeof item === 'string'));
    const optional = (key, validate) => !Object.hasOwn(action, key) || validate(action[key]);
    let valid;
    switch (action.op) {
      case 'managedHttp': valid = managedHttp(action); break;
      case 'http': valid = integer(action.request) && ['GET', 'POST', 'PUT', 'DELETE', 'PATCH'].includes(action.method)
        && typeof action.url === 'string' && headers(action.headers) && optional('body', (v) => typeof v === 'string'); break;
      case 'sign': valid = integer(action.attempt) && object(action.work) && Number.isFinite(action.timeoutMs); break;
      case 'wsSend': valid = integer(action.socket) && typeof action.text === 'string'; break;
      case 'wsConnect': valid = integer(action.socket) && typeof action.url === 'string' && optional('headers', headers); break;
      case 'wsClose': valid = integer(action.socket); break;
      case 'setTimer': valid = integer(action.timer) && Number.isFinite(action.at); break;
      case 'clearTimer': valid = integer(action.timer); break;
      case 'event': valid = ['debug', 'info', 'warn', 'error'].includes(action.level)
        && typeof action.code === 'string' && typeof action.message === 'string' && optional('fields', object); break;
      case 'done': {
        terminalCount++;
        const result = Object.hasOwn(action, 'result');
        const error = Object.hasOwn(action, 'error');
        valid = integer(action.command) && result !== error
          && optional('result', (v) => typeof v === 'string') && optional('error', (v) => typeof v === 'string') && optional('detail', object);
        break;
      }
      case 'commandError': valid = integer(action.command) && typeof action.message === 'string'; break;
      case 'commandProgress': valid = integer(action.command) && Array.isArray(action.receipts); break;
      case 'accountAuth': valid = ['primary', 'secondary'].includes(action.credential)
        && ['account', 'balance', 'orders', 'positions', 'settings'].includes(action.scope)
        && ['accepted', 'rejected'].includes(action.outcome)
        && Object.keys(action).every((key) => ['op', 'credential', 'scope', 'outcome'].includes(key)); break;
      case 'startupFailed': valid = object(action.detail); break;
      case 'deactivate': valid = true; break;
      default: valid = false;
    }
    assert.ok(valid, `${label}: malformed or unknown ${action.op} action`);
    assert.ok(terminalCount <= 1, `${label}: duplicate terminal output`);
    assert.notEqual(action.detail?.kind, 'unsupported', `${label}: advertised handler is unsupported`);
    assert.notEqual(action.code, 'command.unsupported', `${label}: advertised handler is unsupported`);
    if (['done', 'commandError', 'commandProgress'].includes(action.op)) {
      assert.equal(action.command, command.command, `${label}: uncorrelated command output`);
      // Legacy operation-level refusals only. An unsupported symbol/argument is
      // a precondition, not proof that the advertised command is unavailable.
      assert.doesNotMatch(action.error ?? action.message ?? '', /^(?:unsupported|not supported|not implemented)(?:$| command\b)|\b(?:command|operation|query|method|action)\b.*\b(?:unsupported|not supported|not implemented)\b/i,
        `${label}: advertised handler is unsupported`);
      assert.doesNotMatch(action.error ?? action.message ?? '',
        new RegExp(`\\b${command.op}\\b.*\\b(?:unsupported|not supported|not implemented)\\b`, 'i'),
        `${label}: advertised method is unsupported`);
    }
  }
  // Fresh instance per probe: no startup/earlier command can supply these actions.
  // This result is ingress evidence, not valid-state positive handler proof.
  const terminal = actions.find((action) => action.op === 'done');
  const evidence = terminal ? (Object.hasOwn(terminal, 'error') ? 'precondition' : 'terminal')
    : actions.some((action) => ['http', 'sign', 'wsSend'].includes(action.op)
      || (action.op === 'managedHttp' && ['prepare', 'bind'].includes(action.phase))) ? 'request' : 'deferred';
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
  // TxFlow classifies public-only trading as Unsupported. Reach private
  // command ingress with a valid testnet setup, never a refusal exemption.
  // A 0x01-prefixed scalar is nonzero and below the secp256k1 order. This key
  // exists only in this short-lived test process; it is never stored or logged.
  if (venue === 'txflow') Object.assign(base, {
    publicDataOnly: false,
    secret: `0x01${randomBytes(31).toString('hex')}`,
    applicationId: `0x${randomBytes(20).toString('hex')}`,
  });
  if (venue === 'breakoutprop') Object.assign(base, { key: 'user', secret: 'password' });
  if (venue === 'hyrotrader') base.endpoints = {
    rest: 'https://hyro.test', publicWs: 'wss://hyro.test/public', privateWs: 'wss://hyro.test/private',
  };
  return venue === 'bybit' ? [base, { ...base, enableOptionMarkets: true }] : [base];
}
