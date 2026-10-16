import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import vm from 'node:vm';
import ts from 'typescript';
const require = createRequire(import.meta.url);
const root = new URL('../../../packages/safe-cex/src/', import.meta.url);
const exports = {};
vm.runInNewContext(ts.transpileModule(readFileSync(new URL('utils/get-key.ts', root), 'utf8'), {
  compilerOptions: { module: ts.ModuleKind.CommonJS, esModuleInterop: true },
}).outputText, { exports, require });
const text = readFileSync(new URL('exchanges/bybit/bybit.exchange.ts', root), 'utf8');
const ast = ts.createSourceFile('bybit.ts', text, ts.ScriptTarget.Latest, true);
const owner = ast.statements.find(n => ts.isClassDeclaration(n) && n.name?.text === 'BybitExchange');
const methods = owner.members.filter(n => n.name?.getText(ast) === 'inferProtectiveOrderSide');
assert.equal(methods.length, 1);
const subject = vm.runInNewContext(ts.transpileModule(`new class { ${methods[0].getText(ast)} }`, {}).outputText,
  { v: exports.v, OrderSide: { Buy: 'buy', Sell: 'sell' }, PositionSide: { Long: 'long' } });
const orders = [null, {}, [], false, 7, 'BTC', {symbol:'BTC'}, {symbol:''}, {symbol:7},
  ...[1,2,'1','2',true,null,[],[1],[2],{}, {toString:null}].map(positionIdx=>({positionIdx,symbol:'BTC'})),
  {position_idx:'2',symbol:'BTC'}, {positionIdx:null,position_idx:1,symbol:'BTC'}];
const p = (symbol, side, contracts) => ({symbol,side,contracts});
const lists = [[], [p('BTC','long',1)], [p('BTC','short',-2)],
  [p('BTC','long',1),p('BTC','short',2)], [p('OTHER','long',1),p('BTC','short',3)],
  ...[0,-0,NaN,Infinity,-Infinity].map(n=>[p('BTC','long',n)]), [p('','long',1)]];
const cases=[];
for(const order of orders) for(const positions of lists) {
  let reads=0;
  subject.store={get positions(){reads++;return positions;}};
  let expected=null,error=null,message=null;
  try {expected=subject.inferProtectiveOrderSide(order)??null;} catch(e){error=e.name;message=e.message;}
  cases.push({order,positions:positions.map(p=>({...p,contracts:String(p.contracts)})),expected,error,message,reads});
}
assert.equal(cases.length,242,'PROTECTIVE source total');
// The source early return must not even access a hostile symbol getter.
const hostile=[1,2].map(positionIdx=>{
  const witness={positionIdx,invocations:0,reads:0,result:null};
  const record=result=>{witness.invocations++;witness.result=result;};
  record(subject.inferProtectiveOrderSide({positionIdx,get symbol(){witness.reads++;throw Error('eager symbol');}}));
  return witness;
});
const boundaries = [p('BTC','unexpected',2),p('BTC','long','2')].map(position => { subject.store={positions:[position]}; return {position,expected:subject.inferProtectiveOrderSide({symbol:'BTC'})}; });
subject.store={positions:[p('\ud800','long',1)]};
const surrogateWitness={invocations:0,result:null};
const recordSurrogate=result=>{surrogateWitness.invocations++;surrogateWitness.result=result;};
recordSurrogate(subject.inferProtectiveOrderSide({symbol:'\ud800'}));
const surrogate = JSON.stringify({symbol:'\ud800'});
process.stdout.write(JSON.stringify({surrogate,surrogateWitness,hostile,boundaries,capture:{architecture:process.arch,node:process.versions.node},cases}));
