// Live source capture: validated Node 26.7.0 on darwin/arm64; CI executes on its own host.
import fs from 'node:fs';
import vm from 'node:vm';
import crypto from 'node:crypto';
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
const require=createRequire(import.meta.url),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const entry=require.resolve(process.env.SAFE_CEX_WORKER_LODASH_PATH?process.env.SAFE_CEX_WORKER_LODASH_PATH+'/lodash.js':'lodash');
assert.equal(crypto.createHash('sha256').update(fs.readFileSync(entry)).digest('hex'),'f5465f55566bf544aad0a31c6135889ca1ed81eea8f53ec61c6cbe86926f07cf');
const lodash=require(entry);assert.equal(lodash.VERSION,'4.18.1');
const root=new URL('../../../packages/safe-cex/src/',import.meta.url);
function ast(path){return ts.createSourceFile(path,fs.readFileSync(new URL(path,root),'utf8'),ts.ScriptTarget.Latest,true);}
function body(path){const a=ast(path);return a.statements.filter(n=>!ts.isImportDeclaration(n)).map(n=>n.getText(a)).join('\n').replace(/\bexport /g,'');}
const types=ast('types.ts'),names=['OrderSide','OrderType','OrderStatus','PositionSide','PositionMode'];
const enums=types.statements.filter(n=>ts.isEnumDeclaration(n)&&names.includes(n.name.text));
assert.equal(enums.length,names.length);
let events=[],caught=[];
const context=vm.createContext({Error,structuredClone,uniqBy:lodash.uniqBy,isEqual:lodash.isEqual,Date:{now:()=>10000},console:{error(...args){caught.push(args.find(v=>v instanceof Error));},warn(){},log(){}}});
function run(source){vm.runInContext(ts.transpileModule(source,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);}
const clockAst=ast('utils/virtualClock.ts');
const clock=clockAst.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text==='VirtualClock');assert.ok(clock);
run(clock.getText(clockAst)+'\nconst virtualClock=new VirtualClock();\n'+enums.map(n=>n.getText(types).replace('export ','')).join('\n')+'\n'+['utils/clone.ts','store/store.base.ts'].map(body).join('\n')+'\nglobalThis.Store=DefaultStore;');
for(const name of ['BaseHandler','UiStateHandler']){
 run(`(()=>{${body(`webworker/handlers/${name}.ts`)}\nglobalThis.${name}=${name};})();`);
}
// Separate cleanup-order supplement: original 32-case oracle remains byte-identical.
const store=new context.Store();
store.update({positions:[],fetchedLeverage:{OTHER:{buyLeverage:7,sellLeverage:8}}},true);
const publications=[];let calls=0,pending;
store.subscribe(()=>publications.push(Object.keys(store.fetchedLeverage)));
const exchange={store,fetchLeverageForSymbol(symbol){
 assert.equal(symbol,'BTC','MARGIN cleanup source symbol');calls++;
 store.update({fetchedLeverage:{...store.fetchedLeverage,'42':{buyLeverage:21,sellLeverage:23}}},true);
 return {then(callback){return {catch(onError){pending=Promise.resolve(null).then(callback).catch(onError);return pending;}};}};
}};
const handler=new context.UiStateHandler('a','bybit',exchange,{}, {},{fetchLeverageOnMarginSymbolChange:true});
handler.handleActiveMarginSymbolChange({activeMarginSymbol:'BTC'});
await pending;
assert.equal(calls,1,'MARGIN cleanup source fetch count');
assert.equal(publications.length,3,'MARGIN cleanup source publication count');
assert.deepEqual(publications[0],['OTHER','BTC'],'MARGIN cleanup source placeholder keys');
assert.deepEqual(publications[1],['42','OTHER','BTC'],'MARGIN numeric keys enumerate first before cleanup');
assert.deepEqual(publications[2],['42','OTHER'],'MARGIN cleanup source final keys');
process.stdout.write(JSON.stringify({architecture:process.arch,nodeMajor:process.versions.node.split('.')[0],calls,publications,fetched:store.fetchedLeverage}));
