'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const crypto = require('node:crypto');
assert.ok(process.env.SAFE_CEX_TYPESCRIPT_PATH, 'SAFE_CEX_TYPESCRIPT_PATH is required');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const root = process.env.SAFE_CEX_REPO_ROOT || path.resolve(__dirname, '../../..');
const sourceRoot = path.join(root, 'packages/safe-cex/src/exchanges/kraken');
const hashes = {};
function member(file, className, name) {
  const source = fs.readFileSync(path.join(sourceRoot,file), 'utf8');
  hashes[file] = crypto.createHash('sha256').update(source).digest('hex');
  const ast = ts.createSourceFile(file, source, ts.ScriptTarget.Latest, true);
  const classes = ast.statements.filter(node => ts.isClassDeclaration(node) && node.name.text === className);
  assert.equal(classes.length, 1);
  const members = classes[0].members.filter(node => node.name?.getText(ast) === name);
  assert.equal(members.length, 1);
  assert.ok(ts.isArrowFunction(members[0].initializer));
  return { ast, node: members[0].initializer };
}
function compile(source, globals) {
  const result = ts.transpileModule(source, {compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS},reportDiagnostics:true});
  assert.equal((result.diagnostics || []).filter(d => d.category === ts.DiagnosticCategory.Error).length,0);
  return vm.runInNewContext(result.outputText, globals);
}
(async () => {
  const {ast,node} = member('krakenspot.exchange.ts','KrakenSpotExchange','placeOrder');
  const spreads = [];
  function walk(current) {
    if(ts.isArrayLiteralExpression(current) && current.elements.length === 1 && ts.isSpreadElement(current.elements[0]) && current.elements[0].expression.getText(ast) === 'txids') spreads.push(current);
    ts.forEachChild(current,walk);
  }
  walk(node);
  assert.equal(spreads.length,1,'select the actual live txids spread inside placeOrder');
  const spread = compile(`(txids => (${spreads[0].getText(ast)}))`,{});
  const original = {text:'\ud800'};
  const set = spread(new Set([original,Infinity]));
  assert.equal(set.length,2); assert.equal(set[0],original); assert.equal(set[1],Infinity);
  const map = spread(new Map([['id',original]]));
  assert.equal(map.length,1); assert.equal(map[0][0],'id'); assert.equal(map[0][1],original);
  assert.equal(spread('\ud83d\ude00\ud800').length,2);
  assert.ok(Object.is(spread([-0])[0],-0));
  let gets=0,calls=0,iteratorGets=0;
  const iterator = {[Symbol.iterator]() {iteratorGets++;return this;}};
  Object.defineProperty(iterator,'next',{configurable:true,get() {
    gets++;
    return function () {assert.equal(this,iterator);assert.equal(arguments.length,0);return {done:calls++ !== 0,value:original};};
  }});
  const values=spread(iterator);
  assert.equal(values.length,1);assert.equal(values[0],original);
  assert.equal(gets,1);assert.equal(calls,2);assert.equal(iteratorGets,1);
  const error = new RangeError('next getter failed');
  Object.defineProperty(iterator,'next',{get() {throw error;}});
  assert.throws(()=>spread(iterator),actual=>actual===error);
  for(const bad of [null,undefined,true,{}]) assert.throws(()=>spread(bad),actual=>actual.name==='TypeError' && actual.message==='txids is not iterable');
  const tick = member('kraken.exchange.ts','KrakenExchange','tick');
  const emissions=[];
  for(const message of ['original message','']) {
    const receiver={isDisposed:false,async fetchLeverageSettings(){throw {message};},emitAndLogError(...args){emissions.push(args);this.isDisposed=true;}};
    const arrow=compile(`function capture(){return (${tick.node.getText(tick.ast)});} capture;`,{loop(){assert.fail('disposed after emission');}}).call(receiver);
    await arrow(0);
  }
  assert.deepEqual(emissions,[['original message'],['Tick data fetch failed']]);
  console.log(JSON.stringify({hashes,spreadCases:10,originalReferences:true,cachedNext:gets,singleIteratorLookup:true,tickMessages:emissions}));
})().catch(error=>{console.error(error);process.exitCode=1;});
