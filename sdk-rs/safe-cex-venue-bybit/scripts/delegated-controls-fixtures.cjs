const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const root = path.resolve(__dirname, '../../..');
const file = path.join(root, 'packages/safe-cex/src/exchanges/bybit/bybit.exchange.ts');
const ast = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
const owner = ast.statements.find(n => ts.isClassDeclaration(n) && n.name.text === 'BybitExchange');
const names = ['setLeverage', 'changePositionModeHelper'];
const members = names.map(name => {
  const found = owner.members.filter(n => n.name?.getText(ast) === name);
  assert.equal(found.length, 1, name);
  return found[0].getText(ast);
}).join('\n');
const flush = () => new Promise(setImmediate);
async function main() {
  const profiles = [];
  const success = 'All symbols switched successfully.';
  const partial = 'Partial symbols switched successfully, excluding symbols with open orders or positions.';
  for (const kind of ['leverage', 'mode']) for (const selected of [false, true]) {
    for (const reply of kind === 'leverage' ? ['OK'] : [success, partial, 'refused']) {
      const context = vm.createContext({ v: (data, key) => data[key], ENDPOINTS: {
        SET_LEVERAGE: '/v5/position/set-leverage', SET_POSITION_MODE: '/v5/position/switch-mode',
      }});
      vm.runInContext(ts.transpileModule(`class Subject { ${members} } globalThis.api = new Subject();`, {
        compilerOptions: { target: ts.ScriptTarget.ES2022 },
      }).outputText, context);
      const api = context.api, trace = [], checkpoints = {};
      let initial, post, all, symbol;
      const first = [{symbol: 'BTCUSDT', leverage: 2}];
      const refreshed = [{symbol: 'BTCUSDT', leverage: -0, opaque: '\ud800'}];
      const market = {limits: {leverage: {min: 1, max: 5}}};
      api.refuseOptionSymbol = () => {};
      api.store = { marketsBySymbol: {BTCUSDT: market}, positions: selected && kind === 'leverage' ? first : [],
        addOrUpdatePositions(rows) {
          assert(rows === first || rows === refreshed, 'original fetched allocation');
          trace.push(rows === first ? 'upsert-initial' : 'upsert-refresh');
        },
        update(value) { assert.equal(value.positions, refreshed); trace.push('update'); },
      };
      api.getCategoryForSymbol = value => { assert.equal(value, 'BTCUSDT'); return 'linear'; };
      api.fetchPositions = value => {
        if (kind === 'leverage' && !selected && !initial) {
          trace.push('fetch-initial'); return new Promise(resolve => { initial = resolve; });
        }
        trace.push(value ? 'fetch-symbol' : 'fetch-all');
        return new Promise(resolve => { if (kind === 'mode' && value) symbol = resolve; else all = resolve; });
      };
      api.xhr = {post(endpoint, payload) {
        if (kind === 'leverage') {
          assert.equal(endpoint, '/v5/position/set-leverage');
          assert.equal(payload.buyLeverage, selected ? '5' : '4');
          assert.equal(payload.sellLeverage, payload.buyLeverage);
          assert.equal(payload.symbol, 'BTCUSDT');
          trace.push(`post-${payload.buyLeverage}`);
        } else {
          assert.equal(endpoint, '/v5/position/switch-mode');
          assert.equal(payload.mode, 3); assert.equal(payload.category, 'linear');
          assert.equal(payload.coin, 'USDT'); assert.equal(payload.symbol, selected ? 'BTCUSDT' : undefined);
          trace.push('post-mode');
        }
        return new Promise(resolve => { post = resolve; });
      }};
      api.emitAndLogError = message => trace.push(`emit-${message}`);
      const pending = kind === 'leverage' ? api.setLeverage('BTCUSDT', 7)
        : api.changePositionModeHelper(true, 'linear', 'USDT', selected ? 'BTCUSDT' : undefined);
      const result = pending.then(() => null, error => error.message);
      checkpoints.prefix = [...trace];
      if (initial) { market.limits.leverage.max = 4; initial(first); }
      await flush(); checkpoints.afterInitial = [...trace];
      assert(post, 'eager source prefix reaches its first request');
      post({data: {retMsg: reply}}); await flush(); checkpoints.afterPost = [...trace];
      assert(all, 'dropped caller interest does not cancel refresh');
      all(refreshed); await flush(); checkpoints.afterAll = [...trace];
      if (symbol) symbol(refreshed);
      await flush(); checkpoints.settled = [...trace];
      assert.equal(await result, reply === 'refused' ? 'refused' : null);
      profiles.push({kind, selected, reply, checkpoints});
    }
  }
  assert.equal(profiles.length, 8);
  console.log(JSON.stringify(profiles));
}
main().catch(error => { console.error(error); process.exitCode = 1; });
