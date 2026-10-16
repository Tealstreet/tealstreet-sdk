// Live manager supplement; constructor fixtures supply state, so no placement/timer runs.
import fs from 'node:fs';
import vm from 'node:vm';
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
const require=createRequire(import.meta.url),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const root=new URL('../../../packages/safe-cex/src/',import.meta.url);
function source(path){const ast=ts.createSourceFile(path,fs.readFileSync(new URL(path,root),'utf8'),ts.ScriptTarget.Latest,true);return ast.statements.filter(s=>!ts.isImportDeclaration(s)).map(s=>s.getText(ast)).join('\n').replace(/\bexport /g,'');}
const context=vm.createContext({Date:{now:()=>123},nanoid:()=> 'generated'});
vm.runInContext(ts.transpileModule(['webworker/twap/TwapStatus.ts','webworker/twap/TwapManager.ts','webworker/twap/TwapInstance.ts'].map(source).join('\n')+'\nglobalThis.Manager=TwapManager;globalThis.Instance=TwapInstance;globalThis.singleton=twapManager;', {compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);
const market={id:'market-id',symbol:'MARKET',base:'BASE',quote:'QUOTE',active:true,precision:{amount:0.01,price:0.1},limits:{amount:{min:0,max:100},leverage:{min:1,max:100}}};
function instance(id,size,status){
 const options={accountName:'options-account',activeMarket:structuredClone(market),size,duration:17,ordersCount:3,side:'buy',chaserDelay:100,reduceOnly:false,randomnessPercentage:5,useLimitOrders:false,useChaserOrders:false};
 const state={accountName:'state-account',activeMarket:{...structuredClone(market),id:'state-market-id'},size:size+1,ordersSizes:[size+2,size+3],side:'sell',status,ordersCount:4,ordersExecuted:0,nextOrderAt:size+9};
 const record=new context.Instance({}, {postMessage(){}},options,()=>{},state,id);
 record.id=id; // The manager reads the live public ID, including one changed after construction.
 return record;
}
const bytes=new DataView(new ArrayBuffer(8));
function bits(n){bytes.setFloat64(0,n);return bytes.getBigUint64(0).toString(16).padStart(16,'0');}
const queries=['A','B','C','7','42','constructor','__proto__','missing','','é🪙'];
const manager=new context.Manager(),rows=[];
function snapshot(label){
 const out=manager.serializeState();
 rows.push({label,mapKeys:[...manager.runningTwaps.keys()],queries:queries.map(id=>{const found=manager.getById(id);return [id,manager.hasId(id),found?found.id:null];}),entries:Object.keys(out).map(key=>{const {options:o,state:s}=out[key];return {key,account:o.accountName,market:o.activeMarket.id,size:bits(o.size),duration:bits(o.duration),stateAccount:s.accountName,stateMarket:s.activeMarket.id,stateSize:bits(s.size),status:s.status,executed:bits(s.ordersExecuted),next:bits(s.nextOrderAt),sizes:s.ordersSizes.map(bits)};})});
}
snapshot('empty');
let a=instance('A',11,'idle');const b=instance('B',22,'completed'),c=instance('C',33,'paused');
assert.equal(manager.add(a),'A');snapshot('add-A');
assert.equal(manager.add(b),'B');snapshot('add-completed-B');
assert.equal(manager.add(c),'C');snapshot('add-paused-C');
a=instance('A',44,'running');assert.equal(manager.add(a),'A');snapshot('replace-A');
const getIdentity=manager.getById('A')===a;
manager.remove('B');manager.remove('missing');snapshot('remove-B-and-missing');
const held=manager.serializeState();a.options.size=99;a.state.ordersExecuted=7;a.state.nextOrderAt=-0;
const alias={optionSize:bits(held.A.options.size),executed:bits(held.A.state.ordersExecuted),next:bits(held.A.state.nextOrderAt)};
a.state={...a.state,ordersExecuted:9};alias.heldAfterReplace=bits(held.A.state.ordersExecuted);alias.currentAfterReplace=bits(a.state.ordersExecuted);snapshot('alias-and-state-replace');
a.id='42';snapshot('live-ID-change');
manager.add(instance('7',55,'idle'));snapshot('numeric-order');
manager.add(instance('constructor',66,'running'));snapshot('own-constructor');
manager.add(instance('__proto__',77,'idle'));snapshot('prototype-write');
const protoOut=manager.serializeState(),prototype={own:Object.hasOwn(protoOut,'__proto__'),inheritedOptionSize:bits(Object.getPrototypeOf(protoOut).options.size)};
manager.remove('C');snapshot('remove-middle-C');
manager.add(instance('C',88,'idle'));snapshot('reinsert-C');
manager.add(instance('',90,'idle'));snapshot('empty-ID');
manager.add(instance('é🪙',101,'paused'));snapshot('unicode-ID');
assert.equal(rows.length,15,'TWAP-MANAGER source trace count');
const surrogateManager=new context.Manager(),lone=JSON.parse('"\\ud800"');const loneInstance=instance(lone,1,'idle');surrogateManager.add(loneInstance);
const surrogate={getSame:surrogateManager.getById(lone)===loneInstance,has:surrogateManager.hasId(lone),idJson:JSON.stringify(lone),serializedJson:JSON.stringify(surrogateManager.serializeState())};
surrogateManager.remove(lone);surrogate.removed=!surrogateManager.hasId(lone);surrogate.afterRemoveMissing=surrogateManager.getById(lone)===undefined;
const serializedJson=JSON.stringify(manager.serializeState());
const singletonIdentity=vm.runInContext('twapManager === globalThis.singleton',context);
process.stdout.write(JSON.stringify({architecture:process.arch,nodeMajor:Number(process.versions.node.split('.')[0]),rows,getIdentity,alias,prototype,surrogate,singletonIdentity,serializedJson}));
