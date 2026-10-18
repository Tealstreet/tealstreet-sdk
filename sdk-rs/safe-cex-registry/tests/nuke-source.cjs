// Maintained shared nuke methods and sizing utility, executed without rewriting their bodies.
const assert=require('node:assert/strict');
const fs=require('node:fs');
const path=require('node:path');
const root=path.resolve(__dirname,'../../..');
const tsPath=process.env.SAFE_CEX_TYPESCRIPT_PATH||path.join(root,'sdk-rs/target/exchange-typescript-deps/node_modules/typescript');
const ts=require(tsPath);
const {forEachSeries}=require('node:module').createRequire(require.resolve(tsPath))('p-iteration');
const source=fs.readFileSync(path.join(root,'packages/safe-cex/src/exchanges/base.ts'),'utf8');
const ast=ts.createSourceFile('base.ts',source,ts.ScriptTarget.Latest,true);
const cls=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text==='BaseExchange');
const printer=ts.createPrinter();
const names=['closeAllPositionsAtMarket','nuke','nukePositions'];
const members=names.map(name=>{const n=cls.members.find(m=>m.name?.getText(ast)===name);assert.ok(n,name);return printer.printNode(ts.EmitHint.Unspecified,n,ast);});
function compile(text,bindings={}){const exports={};new Function('exports',...Object.keys(bindings),ts.transpileModule(text,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText)(exports,...Object.values(bindings));return exports;}
const sizing=compile(fs.readFileSync(path.join(root,'packages/safe-cex/src/utils/nativePositionSize.ts'),'utf8'));
const {Probe}=compile(`export class Probe {${members.join('\n')}}`,{forEachSeries,nativePositionSize:sizing.nativePositionSize,OrderSide:{Sell:'sell',Buy:'buy'},OrderType:{Market:'market'},LogSeverity:{Error:'error'},sleep:()=>{throw Error('tries=3 must not sleep');}});
const pos=(symbol='BTCUSDT',contracts=3,notional=150000,extra)=>({symbol,contracts,notional,side:'long',...(extra?{extra}: {})});
function fixture(positions,market,cancelError=false){const ex=new Probe(),calls=[],logs=[];Object.assign(ex,{isDisposed:false,isNuking:false,store:{positions,marketsBySymbol:{BTCUSDT:market}},cancelAllOrders:async()=>{calls.push({cancel:true});if(cancelError)throw Error('cancel refused');},placeOrder:async order=>{calls.push(order);return[];},log:(message,severity)=>logs.push({message,severity})});return{ex,calls,logs};}
(async()=>{
 let cases=0;
 for(const [inverse,size,contracts,notional,native,expected]of [[false,0.0001,0.0012,60,undefined,12],[true,100,0.2,10000,90,100],[false,0.01,0.2,10000,30,30]]){
  const h=fixture([pos('BTCUSDT',contracts,notional,native===undefined?undefined:{contractsNative:native})],{contractSize:size,isInverse:inverse});await h.ex.nukePositions(3);
  assert.equal(h.calls.length,1);assert.ok(Math.abs(h.calls[0].amount-expected)<1e-12);assert.equal(h.calls[0].side,'sell');assert.equal(h.calls[0].type,'market');assert.equal(h.calls[0].reduceOnly,true);assert.equal(h.ex.isNuking,false);assert.equal(h.ex.store.positions.length,1);cases++;
 }
 for(const fails of [false,true]){const h=fixture([pos()],{contractSize:1},fails);await h.ex.nuke(3);assert.equal(h.calls.length,2);assert.deepEqual(h.calls[0],{cancel:true});assert.equal(h.calls[1].amount,3);assert.equal(h.ex.isNuking,false);assert.equal(h.logs.length,fails?1:0);if(fails)assert.deepEqual(h.logs,[{message:'Nuke: failed to cancel orders: cancel refused',severity:'error'}]);cases++;}
 const h=fixture([pos('UNLOADED',2,1),pos()],{contractSize:1});await h.ex.nukePositions(3);assert.equal(h.calls.length,1);assert.equal(h.calls[0].symbol,'BTCUSDT');assert.deepEqual(h.logs,[{message:'Nuke: failed to close position on UNLOADED: market not loaded',severity:'error'}]);cases++;
 console.log(`Actual TS nuke: ${cases} cases PASS (three sizes, cancel order/failure, missing-market siblings).`);
})().catch(e=>{console.error(e);process.exitCode=1;});
