import fs from 'node:fs';
import vm from 'node:vm';
import assert from 'node:assert/strict';
import {ts} from '../../safe-cex-store/tests/source_loader.mjs';
const url=new URL('../../../packages/safe-cex/src/exchanges/okx/okx.exchange.ts',import.meta.url);
const ast=ts.createSourceFile(url.pathname,fs.readFileSync(url,'utf8'),ts.ScriptTarget.Latest,true);
const owner=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text==='OKXExchange');
const names=['getTicker','toFiniteNumber','resolveReferencePrice','hasExternalTriggerReferencePrice','resolveTriggerReferencePrice'];
const members=names.map(name=>{const member=owner.members.find(n=>n.name?.getText(ast)===name);assert.ok(member,'maintained '+name);return member.getText(ast);});
const module={exports:{}};
const code=ts.transpileModule('export class Selected {'+members.join('\n')+'}',{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
vm.runInThisContext('(function(module,exports){'+code+'\n})')(module,module.exports);
const input=JSON.parse(fs.readFileSync(0,'utf8'));
assert.equal(input.lookups.length,24);assert.equal(input.reference.length,12);assert.equal(input.external.length,42);assert.equal(input.trigger.length,18);
const decode=bits=>bits===null?undefined:Buffer.from(bits,'hex').readDoubleBE();
const units=text=>Array.from({length:text.length},(_,i)=>text.charCodeAt(i));
const text=values=>String.fromCharCode(...values);
const failure=()=>new TypeError('reference failure');
function outcome(value,error){if(error)return {error:{name:error.name,message:error.message,same:true}};const b=Buffer.alloc(8);b.writeDoubleBE(value);return {bits:b.toString('hex')};}
function ticker(values,trace,error,failField,mutateLast){
 const data={...values}, result={};
 for(const field of ['mark','index','last'])Object.defineProperty(result,field,{get(){
  trace.push('read:'+field);if(field===failField)throw error;
  if(field==='mark'&&mutateLast!==undefined)data.last=mutateLast;
  return data[field];
 }});
 return {result,data};
}
const lookups=[];
for(const c of input.lookups){
 const receiver=new module.exports.Selected(),trace=[],error=failure(),id={value:7},symbol={value:13};
 const byId=Object.create(null),bySymbol=Object.create(null);
 Object.defineProperty(byId,text(c.key),{get(){trace.push('id');if(c.failId)throw error;return c.idHit?id:undefined;}});
 Object.defineProperty(bySymbol,text(c.symbol),{get(){trace.push('symbol');return c.symbolHit?symbol:undefined;}});
 receiver.store={tickersById:byId,tickersBySymbol:bySymbol};let result;
 try{const value=receiver.getTicker({id:text(c.key),symbol:text(c.symbol)});assert.equal(value,c.idHit?id:c.symbolHit?symbol:undefined);result={value:value?.value??null};}
 catch(e){assert.equal(e,error,'lookup original failure');result={error:true};}
 lookups.push({trace,outcome:result});
}
const reference=[];
for(const c of input.reference){
 const receiver=new module.exports.Selected(),trace=[],error=failure();
 const {result}=ticker({mark:decode(c.mark),last:decode(c.last)},trace,error,c.failField,decode(c.mutateLast));
 receiver.getTicker=()=>{trace.push('get');return result;};let value,caught;
 try{value=receiver.resolveReferencePrice({},decode(c.reference));}catch(e){assert.equal(e,error,'reference original failure');caught=e;}
 reference.push({trace,outcome:outcome(value,caught)});
}
const external=[];
for(const c of input.external){
 const receiver=new module.exports.Selected(),trace=[],error=failure();
 const t=ticker({mark:decode(c.price),index:decode(c.price),last:decode(c.last)},trace,error);
 const source={};Object.defineProperty(source,c.source==='mark'?'hasMarkPriceLoaded':'hasIndexPriceLoaded',{get(){
  trace.push('external:'+c.source);if(c.throws)throw error;
  if(c.mutate!==null)t.data[c.source]=decode(c.mutate);
  return c.reported===null?undefined:marketId=>{assert.deepEqual(units(marketId),[0xd800,77]);return c.reported;};
 }});
 receiver.options={externalPublicData:c.config===0?undefined:c.config===1?{}:{type:'in-process',source}};
 let result;
 try{result={value:receiver.hasExternalTriggerReferencePrice({id:text([0xd800,77])},c.source,t.result)};}
 catch(e){assert.equal(e,error,'external original failure');result={error:true};}
 external.push({trace,outcome:result});
}
const trigger=[];
for(const c of input.trigger){
 const receiver=new module.exports.Selected(),trace=[],error=failure();
 const first=ticker({mark:11,index:7,last:decode(c.last)},trace,error,c.failure==='read'?'mark':undefined);
 if(c.override!==null)first.data[text(c.kind)==='index'?'index':'mark']=decode(c.override);
 const second=ticker({mark:17,index:19,last:29},trace,error);let gets=0;
 receiver.getTicker=()=>{const i=gets++;trace.push('get:'+i);if(c.failure==='get:'+i)throw error;return i===0?first.result:second.result;};
 receiver.hasExternalTriggerReferencePrice=(market,source,value)=>{
  assert.equal(value,first.result,'external first ticker identity');trace.push('external:'+source);
  if(c.failure==='external')throw error;if(c.failure==='mutate')first.data.index=23;
  return c.external;
 };
 receiver.indexPriceMarketIds=new Set(c.loaded?[text([77,0xd800])]:[]);
 receiver.markPriceMarketIds=new Set(c.loaded?[text([77,0xd800])]:[]);
 let value,caught;
 try{value=receiver.resolveTriggerReferencePrice({id:text([77,0xd800])},text(c.kind),decode(c.fallback));}
 catch(e){assert.equal(e,error,'trigger original failure');caught=e;}
 trigger.push({trace,outcome:outcome(value,caught)});
}
const witness=new module.exports.Selected(),market={id:'M',symbol:'A'},wanted={value:29};
witness.store={tickersById:{get M(){market.symbol=text([66,0xd800]);return undefined;}},tickersBySymbol:{[text([66,0xd800])]:wanted}};
assert.equal(witness.getTicker(market),wanted,'lazy symbol read after ID lookup mutation');
witness.getTicker=()=>undefined;assert.equal(witness.resolveReferencePrice({},undefined),0,'absent ticker');
const changingMarket={id:'A'};witness.indexPriceMarketIds=new Set(['B']);
witness.getTicker=()=>{changingMarket.id='B';return {index:7};};
witness.hasExternalTriggerReferencePrice=()=>assert.fail('external after live ID loaded');
assert.equal(witness.resolveTriggerReferencePrice(changingMarket,'index'),7,'loaded predicate after ticker changes market ID');
process.stdout.write(JSON.stringify({lookups,reference,external,trigger}));
