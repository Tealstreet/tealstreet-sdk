const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const root = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/backpack');
function body(file, className, clearTimeout) {
  const filename = path.join(root, file), text = fs.readFileSync(filename, 'utf8');
  const tree = ts.createSourceFile(filename, text, ts.ScriptTarget.Latest, true);
  const classes = tree.statements.filter(n => ts.isClassDeclaration(n) && n.name?.text === className);
  assert.equal(classes.length, 1, 'exact actual source class');
  const methods = classes[0].members.filter(n => ts.isPropertyDeclaration(n) && n.name?.getText(tree) === 'dispose');
  assert.equal(methods.length, 1, 'exact actual source dispose');
  assert.ok(ts.isArrowFunction(methods[0].initializer), 'source dispose remains an arrow');
  const code = ts.transpileModule(`(function(){ return ${methods[0].initializer.getText(tree)}; })`, {
    compilerOptions: {target: ts.ScriptTarget.ES2022}
  }).outputText;
  return vm.runInNewContext(code, {clearTimeout});
}
function observe(surface, failPublic = false) {
  const calls = [], parent = {isDisposed:false};
  const pub = {
    subscriptions: new Set(['s']), topics: {keep:1}, messageHandlers:{keep:[1]},
    orderBookState:{s:{}}, orderBookSnapshotFetchers:{s:()=>{}},
    depthTickerUpdateThrottles:{s:{cancel(){ calls.push('depth:cancel'); }}},
    handleTickerUpdate:{pending:true,cancel(){ calls.push('ticker:cancel'); }},
    ws:{close(){calls.push(`public:close:${parent.isDisposed}`); if(failPublic) throw new Error('public close');}}
  };
  const priv = {
    subscriptions:new Set(['s']), ordersThrottleTimeout:7,
    positionFetchScheduled:true,hasPendingPositionEvents:true,
    balanceFetchScheduled:true,hasPendingBalanceEvents:true,
    ws:{close(){calls.push(`private:close:${parent.isDisposed}`);}}
  };
  const clearTimeout = id => { assert.equal(id,7); calls.push('orders:clear'); };
  pub.dispose = body('backpack.ws-public.ts','BackpackPublicWebSocket',clearTimeout).call(pub);
  priv.dispose = body('backpack.ws-private.ts','BackpackPrivateWebSocket',clearTimeout).call(priv);
  parent.publicWebsocket=pub; parent.privateWebsocket=priv;
  parent.dispose = body('backpack.exchange.ts','BackpackExchange',clearTimeout).call(parent);
  (surface==='exchange'?parent:surface==='public'?pub:priv).dispose();
  return {
    calls, disposed:parent.isDisposed,
    public:{subscriptions:pub.subscriptions.size,handlers:Object.keys(pub.messageHandlers).length,
      topics:Object.keys(pub.topics).length,states:Object.keys(pub.orderBookState).length,
      fetchers:Object.keys(pub.orderBookSnapshotFetchers).length,depth:Object.keys(pub.depthTickerUpdateThrottles).length,
      tickerPending:pub.handleTickerUpdate.pending},
    private:{subscriptions:priv.subscriptions.size,ordersPending:priv.ordersThrottleTimeout!==undefined,
      positionsScheduled:priv.positionFetchScheduled,positionsPending:priv.hasPendingPositionEvents,
      balanceScheduled:priv.balanceFetchScheduled,balancePending:priv.hasPendingBalanceEvents}
  };
}
const cases=['exchange','public','private'].map(name=>({name, ...observe(name)}));
assert.equal(cases.length,3,'three real source disposal surfaces');
assert.deepEqual(cases[0].calls,['depth:cancel','public:close:true','orders:clear','private:close:true']);
assert.equal(cases[1].public.handlers,1,'source retains public handlers');
assert.equal(cases[2].private.positionsPending,true,'source retains private pending work');
assert.throws(()=>observe('exchange',true),/public close/,'source close failure propagates');
process.stdout.write(JSON.stringify(cases));
