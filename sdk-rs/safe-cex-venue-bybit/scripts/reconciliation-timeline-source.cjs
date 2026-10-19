const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const root = path.resolve(__dirname, '../../..');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const axios = require(path.join(root, 'node_modules/axios')).default;
const { mapSeries } = require(path.join(root, 'node_modules/p-iteration'));
const file = path.join(root, 'packages/safe-cex/src/exchanges/bybit/bybit.exchange.ts');
const ast = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
const cls = ast.statements.find((n) => ts.isClassDeclaration(n) && n.name.text === 'BybitExchange');
const names = [
  'clearRecentlyPlacedOrderReconciliationTimers',
  'scheduleLocalReduceOnlyLimitOrderReconciliation',
  'reconcileLocalReduceOnlyLimitOrder',
  'getFilledOrderReconciliationTarget',
  'fetchFillsForOrderIds',
  'markOrderSeen',
];
const selected = names.map((name) => {
  const members = cls.members.filter((n) => n.name?.getText(ast) === name);
  assert.equal(members.length, 1, name);
  return members[0].getText(ast);
});
assert.match(
  cls.members.find(ts.isConstructorDeclaration).getText(ast),
  /maxRPS:\s*5/,
  'maintained constructor selects five requests per second',
);
const constants = [
  'RECENTLY_PLACED_ORDER_GRACE_MS',
  'REDUCE_ONLY_LIMIT_RECONCILE_DELAYS_MS',
  'FILL_RECONCILIATION_MAX_PAGES',
].map((name) => {
  const nodes = ast.statements.filter(
    (n) => ts.isVariableStatement(n) && n.declarationList.declarations.some((d) => d.name.getText(ast) === name),
  );
  assert.equal(nodes.length, 1, name);
  return nodes[0].getText(ast);
});
const loggingFile = path.join(root, 'packages/logging/src/LogSeverity.ts');
const logging = { exports: {} };
vm.runInNewContext(
  ts.transpileModule(fs.readFileSync(loggingFile, 'utf8'), { compilerOptions: { module: ts.ModuleKind.CommonJS } })
    .outputText,
  logging,
);
const typesFile = path.join(root, 'packages/safe-cex/src/exchanges/bybit/bybit.types.ts');
const typesAst = ts.createSourceFile(typesFile, fs.readFileSync(typesFile, 'utf8'), ts.ScriptTarget.Latest, true);
const endpointDeclaration = typesAst.statements
  .filter(ts.isVariableStatement)
  .flatMap((node) => Array.from(node.declarationList.declarations))
  .find((node) => node.name.getText(typesAst) === 'ENDPOINTS');
const endpoint = endpointDeclaration.initializer.properties.find(
  (node) => node.name?.getText(typesAst) === 'USER_TRADES',
).initializer.text;
assert.equal(endpoint, '/v5/execution/list');
async function run(routed) {
  let now = 0,
    id = 0,
    page = 0;
  const timers = new Map(),
    calls = [],
    logs = [];
  const setTimeout = (fn, ms, ...args) => {
    const next = ++id;
    timers.set(next, { at: now + ms, fn: () => fn(...args) });
    return next;
  };
  const clearTimeout = (id) => timers.delete(id);
  async function drain() {
    for (let turn = 0; turn < 1000; turn++) {
      for (let micro = 0; micro < 100; micro++) await Promise.resolve();
      const due = [...timers].filter(([, timer]) => timer.at <= now).sort((a, b) => a[1].at - b[1].at || a[0] - b[0]);
      if (!due.length) return;
      const [key, timer] = due[0];
      timers.delete(key);
      timer.fn();
    }
    throw Error('virtual timer did not quiesce');
  }
  const limiterModule = { exports: {} };
  vm.runInNewContext(fs.readFileSync(require.resolve(path.join(root, 'node_modules/axios-rate-limit')), 'utf8'), {
    module: limiterModule,
    exports: limiterModule.exports,
    setTimeout,
    clearTimeout,
  });
  const xhr = limiterModule.exports(
    axios.create({
      adapter: async (config) => {
        const cursor = config.params?.cursor ?? '';
        calls.push({ at: now, path: config.url, cursor });
        const index = routed ? (cursor ? Number(cursor.slice(7)) + 1 : 0) : Math.min(page++, 19);
        return {
          status: 200,
          statusText: 'OK',
          headers: {},
          config,
          data: { retCode: 0, retMsg: 'OK', result: { list: [], nextPageCursor: `cursor-${index}` } },
        };
      },
    }),
    { maxRPS: 5 },
  );
  const context = vm.createContext({
    setTimeout,
    clearTimeout,
    mapSeries,
    LogSeverity: logging.exports.LogSeverity,
    ENDPOINTS: { USER_TRADES: endpoint },
    v: (o, k) => o[k],
    virtualClock: { getCurrentTimeMs: () => now },
  });
  const program =
    constants.join('\n') + '\nclass Subject{' + selected.join('\n') + '}\nglobalThis.subject=new Subject();';
  vm.runInContext(
    ts.transpileModule(program, { compilerOptions: { target: ts.ScriptTarget.ES2022 } }).outputText,
    context,
  );
  const subject = context.subject;
  const order = {
    id: 'ro-1',
    symbol: 'BTCUSDT',
    type: 'limit',
    reduceOnly: true,
    amount: 0.01,
    remaining: 0.01,
    filled: 0,
  };
  subject.recentlyPlacedOrders = new Map([['ro-1', { placedAt: 0, order }]]);
  subject.recentlyPlacedOrderReconciliationTimers = new Map();
  subject.isDisposed = false;
  subject.store = {
    orders: [order],
    loaded: {},
    removeOrders(rows) {
      this.orders = this.orders.filter((order) => !rows.some((row) => row.id === order.id));
    },
    update(value) {
      Object.assign(this, value);
    },
  };
  subject.shouldReconcileLocalReduceOnlyLimitOrder = () => true;
  subject.resolveOrderSymbolForFills = () => order.symbol;
  subject.getCategoryForSymbol = () => 'linear';
  subject.log = (message, severity) => logs.push({ at: now, message, severity });
  subject.fetchOrders = async () => [];
  subject.xhr = xhr;
  for (const route of ['/v5/position/list', '/v5/order/create']) {
    let settled = false;
    const pending = xhr.get(route).then(() => {
      settled = true;
    });
    await drain();
    assert(settled);
    await pending;
  }
  page = 0;
  subject.scheduleLocalReduceOnlyLimitOrderReconciliation(order);
  const checkpoints = [];
  for (let at = 0; at <= 8100; at += 50) {
    now = at;
    await drain();
    if ([1100, 2100, 3100, 4100, 5100, 6100, 7100, 8100].includes(at))
      checkpoints.push({
        at,
        calls: calls
          .filter((call) => call.path === '/v5/execution/list')
          .map((call) => ({ at: call.at, cursor: call.cursor })),
        logs: logs.slice(),
        retained: subject.store.orders.some((order) => order.id === 'ro-1'),
      });
  }
  return { routed, checkpoints };
}
async function verifyHyroSnapshotSetup() {
  function members(file, className, names) {
    const source = ts.createSourceFile(
      file,
      fs.readFileSync(path.join(root, file), 'utf8'),
      ts.ScriptTarget.Latest,
      true,
    );
    const cls = source.statements.find((node) => ts.isClassDeclaration(node) && node.name.text === className);
    return names
      .map((name) => {
        const selected = cls.members.filter((node) => node.name?.getText(source) === name);
        assert.equal(selected.length, 1, name);
        return selected[0].getText(source);
      })
      .join('\n');
  }
  const exchange = members('packages/safe-cex/src/exchanges/bybit/hyrotrader.exchange.ts', 'HyrotraderExchange', [
    'reconcilePrivateState',
    'fetchAndSetPrivateSnapshot',
    'fetchOrdersPositionsAndBalance',
  ]);
  const socket = members('packages/safe-cex/src/exchanges/bybit/hyrotrader.ws-private.ts', 'HyroPrivateWebsocket', [
    'resetSession',
  ]);
  const context = vm.createContext({ clearTimeout });
  vm.runInContext(
    ts.transpileModule(
      `class Exchange { ${exchange} } class Socket { ${socket} } globalThis.owner = new Exchange(); globalThis.socket = new Socket();`,
      { compilerOptions: { target: ts.ScriptTarget.ES2022 } },
    ).outputText,
    context,
  );
  const owner = context.owner,
    ws = context.socket;
  Object.assign(ws, { generation: 0, stateRevision: 1, pendingAcks: new Map(), parent: {} });
  let release;
  const held = new Promise((resolve) => {
    release = resolve;
  });
  const positions = [{ leverage: 10, unrealizedPnl: 5 }];
  Object.assign(owner, {
    snapshotVersion: 0,
    ordersSnapshotVersion: 0,
    isDisposed: false,
    privateWebsocket: ws,
    store: {
      positions: [],
      loaded: {},
      update(value) {
        Object.assign(this, value);
      },
    },
    fetchOrders: async () => [],
    fetchPositions: async () => {
      await held;
      return positions;
    },
    fetchRawBalance: async () => ({}),
    formatBalance: () => ({ total: 95, upnl: 5 }),
    emitAndLogError(error) {
      throw error;
    },
  });
  const startup = owner.fetchOrdersPositionsAndBalance();
  ws.resetSession();
  release();
  await startup;
  assert.equal(owner.store.positions.length, 0, 'Open invalidates the pending startup snapshot');
  await owner.fetchAndSetPrivateSnapshot();
  assert.strictEqual(owner.store.positions, positions, 'explicit snapshot establishes the retained baseline');
  assert.equal(owner.store.positions[0].leverage, 10);
  assert.equal(owner.store.balance.total, 95);
}
(async () => {
  await verifyHyroSnapshotSetup();
  console.log(JSON.stringify([await run(false), await run(true)]));
})().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
