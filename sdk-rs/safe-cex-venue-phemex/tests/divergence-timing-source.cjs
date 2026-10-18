const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const venue = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/phemex');
function bind(file, owner, name, receiver, dependencies = {}) {
  const filename = path.join(venue, file);
  const ast = ts.createSourceFile(filename, fs.readFileSync(filename, 'utf8'), ts.ScriptTarget.Latest, true);
  const classes = ast.statements.filter(n => ts.isClassDeclaration(n) && n.name?.text === owner);
  assert.equal(classes.length, 1);
  const members = classes[0].members.filter(n => n.name?.getText(ast) === name);
  assert.equal(members.length, 1);
  const text = `(function(){return ${members[0].initializer.getText(ast)}})`;
  return vm.runInNewContext(ts.transpileModule(text, {compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText, dependencies).call(receiver);
}
function held() {
  let resolve;
  return {promise: new Promise(r => {resolve=r}), release(value) {resolve(value)}};
}
async function run() {
  let initCalls = 0;
  const init = held();
  const startOwner = {initExternalPublicData(){initCalls++;return init.promise}};
  void bind('phemex.exchange.ts','PhemexExchange','start',startOwner)();
  assert.equal(initCalls,1,'source start initializer runs before Promise return');
  const response = held();
  const effects = [];
  const originalError = {code:17};
  const fallback = [];
  const receiver = {
    xhr: {get(endpoint) {effects.push(endpoint); return response.promise}},
    emitAndLogError(error) {assert.equal(error, originalError); effects.push('emit')},
    parseTickers(raw) {assert.equal(raw, null); effects.push('parse'); return fallback},
  };
  receiver.fetchTickersRaw = bind('phemex.exchange.ts', 'PhemexExchange', 'fetchTickersRaw', receiver,
    {ENDPOINTS:{TICKERS:'/md/v3/ticker/24hr/all'}, v:(v,k)=>v[k]});
  const ticker = bind('phemex.exchange.ts', 'PhemexExchange', 'fetchTickers', receiver)();
  assert.deepEqual(effects, ['/md/v3/ticker/24hr/all'], 'source ticker read starts before Promise return');
  response.release({data:{error:originalError}});
  assert.equal(await ticker, fallback, 'source fallback reference preserved');
  assert.deepEqual(effects, ['/md/v3/ticker/24hr/all','emit','parse']);
  for (const mode of ['commit','store-failure','disposed']) {
    const gate = held();
    let reads = 0, commits = 0, updates = 0;
    const balance = {total:200};
    const row = {id:'original'};
    const socket = {positionsAndBalanceSyncTimeout:9, positionsAndBalanceSyncSymbols:new Set(), isDisposed:false,
      parent:{fetchPositionsAndBalance(){reads++;return gate.promise}},
      store:{loaded:{},addOrUpdatePositionsFull(rows){assert.equal(rows[0],row);commits++;if(mode==='store-failure')throw originalError},
        update(patch){assert.equal(patch.balance,balance);assert.equal(patch.loaded.balance,true);assert.equal(patch.loaded.positions,true);updates++}}};
    let waiter = bind('phemex.ws-private.ts','BinancePrivateWebsocket','syncPositionsAndBalance',socket,
      {uniqBy:rows=>rows})();
    assert.equal(reads,1);
    assert.equal(socket.positionsAndBalanceSyncTimeout,undefined);
    assert.equal(socket.positionsAndBalanceSyncSymbols.size,0);
    const completed = waiter.then(value=>{assert.equal(value,undefined)});
    waiter = undefined;
    if(mode==='disposed')socket.isDisposed=true;
    gate.release({balance,positions:[row]});
    await completed;
    assert.equal(commits,mode==='disposed'?0:1);
    assert.equal(updates,mode==='commit'?1:0);
  }
  process.stdout.write(JSON.stringify({ticker:1,reconciliation:3}));
}
run().catch(error=>{console.error(error);process.exitCode=1});
