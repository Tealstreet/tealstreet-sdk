// Audit witness, not a claim that the Rust execution handler is implemented.
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const runtime=require('./oracle-runtime.cjs');
const root=path.resolve(__dirname,'../../../packages/safe-cex/src');
const cache=new Map();
function load(file){
 if(cache.has(file))return cache.get(file);
 let source=fs.readFileSync(file,'utf8');
 if(file===path.join(root,'types.ts')){const ast=ts.createSourceFile(file,source,ts.ScriptTarget.Latest,true);source=ast.statements.filter(ts.isEnumDeclaration).map(n=>n.getText(ast)).join('\n');}
 const exports={};cache.set(file,exports);
 vm.runInNewContext(ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,{exports,require:name=>name.startsWith('.')?load(path.resolve(path.dirname(file),name)+'.ts'):runtime(name)},{filename:file});
 return exports;
}
const file=path.join(root,'exchanges/woo/woo.ws-private.ts');
const ast=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
const cls=ast.statements.find(ts.isClassDeclaration);
const member=cls.members.find(n=>n.name?.getText(ast)==='handleExecutionReport');
assert.ok(member);
const body=ts.transpileModule('class Witness {'+member.getText(ast)+'}\nglobalThis.Witness=Witness;',{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText;
const constants=load(path.join(root,'exchanges/woo/woo.types.ts'));
if(process.argv.includes('--correct-prototype'))Object.setPrototypeOf(constants.ORDER_SIDE,null);
const context={...constants,...load(path.join(root,'types.ts')),...load(path.join(root,'utils/get-key.ts')),...load(path.join(root,'exchanges/woo/woo.utils.ts'))};
vm.runInNewContext(body,context);
const results=[];
for(const side of ['unknown','constructor','toString','valueOf','hasOwnProperty','isPrototypeOf','propertyIsEnumerable','toLocaleString','__proto__','__defineGetter__','__defineSetter__','__lookupGetter__','__lookupSetter__']){
 const emitted=[];const worker=new context.Witness();
 worker.parent={mapLimitOrder:()=>null,emitter:{emit:(name,value)=>emitted.push({name,sideType:typeof value.side})}};
 worker.store={updateOrderStatus(){}};worker.parseTimestampToSeconds=()=>undefined;
 worker.handleExecutionReport({status:'FILLED',side,symbol:'PERP_BTC_USDT',orderId:'1',executedPrice:10,executedQuantity:1});
 assert.equal(emitted.length,side==='unknown'?0:1,'ts-bug: inherited side must bypass missing-side guard: '+side);
 if(emitted.length)assert.equal(emitted[0].sideType,side==='__proto__'?'object':'function');
 results.push({side,emitted});
}
console.log(JSON.stringify(results));

// Execute the whole WOO public onMessage member, including its length guard.
const publicFile=path.join(root,'exchanges/woo/woo.ws-public.ts');
const publicAst=ts.createSourceFile(publicFile,fs.readFileSync(publicFile,'utf8'),ts.ScriptTarget.Latest,true);
const publicClass=publicAst.statements.find(ts.isClassDeclaration);
const message=publicClass.members.find(n=>n.name?.getText(publicAst)==='onMessage');
const handlers=publicClass.members.find(n=>n.name?.getText(publicAst)==='messageHandlers');
const publicContext={...load(path.resolve(root,'../../json-parse/src/jsonParse.ts'))};
vm.runInNewContext(ts.transpileModule('class Witness {'+handlers.getText(publicAst)+'\n'+message.getText(publicAst)+'}\nglobalThis.Witness=Witness;',{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,publicContext);
const dispatch=[];
for(const key of ['unknown','constructor','toString','valueOf','hasOwnProperty','isPrototypeOf','propertyIsEnumerable','toLocaleString','__proto__','__defineGetter__','__defineSetter__','__lookupGetter__','__lookupSetter__']){
 const worker=new publicContext.Witness();worker.trackLastReceived=()=>{};
 if(process.argv.includes('--correct-public-prototype'))Object.setPrototypeOf(worker.messageHandlers,null);
 const stats=[];worker.recordMessageHandlerStats=k=>stats.push(k);
 let error;try{worker.onMessage({data:JSON.stringify({topic:key,data:{}})});}catch(e){error={name:e.name,message:e.message};}
 const throws=['constructor','hasOwnProperty','isPrototypeOf','propertyIsEnumerable','__defineGetter__','__defineSetter__','__lookupGetter__','__lookupSetter__'].includes(key);
 assert.equal(Boolean(error),throws,'ts-bug public handler length/iteration '+key);
 assert.equal(stats.length,throws?1:0);
 if(error)assert.equal(error.name,'TypeError');
 dispatch.push({key,stats,error:error||null});
}
console.log(JSON.stringify({publicOnMessage:dispatch}));
