'use strict';
const {ts, fs, path, vm, root, assert, load} = require('./oracle-loader.cjs');
const file = path.join(root, 'packages/safe-cex/src/exchanges/bitunix/bitunix.exchange.ts');
const ast = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
const cls = ast.statements.find(ts.isClassDeclaration);
const {ENDPOINTS} = load(path.join(root, 'packages/safe-cex/src/exchanges/bitunix/bitunix.types.ts'));
const {MarginType, PositionSide} = load(path.join(root, 'packages/safe-cex/src/types.ts'));
const {createPositionId} = load(path.join(root, 'packages/safe-cex/src/utils/createPositionId.ts'));
function method(name, receiver) {
  const members = cls.members.filter(member => member.name?.getText(ast) === name);
  assert.equal(members.length, 1);
  const code = ts.transpileModule('(function(){return (' + members[0].initializer.getText(ast) + ');})', {
    compilerOptions: {target: ts.ScriptTarget.ES2022},
  }).outputText;
  return vm.runInNewContext(code, {ENDPOINTS, MarginType, PositionSide, createPositionId, console}).call(receiver);
}
async function main() {
  const requests = [];
  const receiver = {
    xhr: {get: async endpoint => {requests.push(endpoint); return {data: {data: []}};}},
    store: {tickers: [], marketsBySymbol: {}},
  };
  void method('fetchTickers', receiver)();
  assert.deepEqual(requests, [ENDPOINTS.TICKERS], 'actual source call prefix survives discarded result');
  const writes = [];
  for (const metadata of [false, true]) for (const held of [false, true]) {
    let release = () => {};
    let sent = 0;
    const response = held ? new Promise(resolve => {release = resolve;}) : Promise.resolve({data:{data:{leverage:23,marginMode:'ISOLATION'}}});
    const positions = ['long', 'short'].map(side => ({id: createPositionId('BTCUSDT', side), symbol:'BTCUSDT', leverage:10}));
    const store = {
      positions,
      marketsBySymbol: {BTCUSDT:{limits:{leverage:{min:1,max:100}}}},
      updatePositions(updates) {
        for (const [selector, patch] of updates) Object.assign(this.positions.find(p => p.id === selector.id), patch);
      },
      update(patch) {Object.assign(this, patch);},
    };
    const owner = {
      store, symbolSettings: new Map(),
      xhr: {
        get: () => {sent++; return response;},
        post: () => {sent++; return response;},
      },
      emitAndLogError: error => {throw new Error(String(error));},
    };
    void method(metadata ? 'updateSymbolMetadata' : 'setLeverage', owner)('BTCUSDT', 23);
    assert.equal(sent, 1, 'source starts transport before any caller await');
    assert.deepEqual(store.positions.map(p => p.leverage), [10,10]);
    release({data:{data:{leverage:23,marginMode:'ISOLATION'}}});
    await response;
    await new Promise(setImmediate);
    assert.deepEqual(store.positions.map(p => p.leverage), [23,23], 'source owns dropped-result continuation');
    if (metadata) assert.equal(owner.symbolSettings.get('BTCUSDT').marginType, MarginType.Isolated);
    writes.push({metadata, held, sent, leverage:store.positions.map(p => p.leverage)});
  }
  process.stdout.write(JSON.stringify({requests,writes}));
}
main().catch(error => {console.error(error); process.exitCode=1;});
