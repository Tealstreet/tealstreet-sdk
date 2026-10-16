'use strict';
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const compiler=process.env.SAFE_CEX_TYPESCRIPT_PATH;assert(compiler&&path.isAbsolute(compiler),'absolute TypeScript compiler required');
const ts=require(compiler),source=path.resolve(__dirname,'../../../packages/safe-cex/src/exchanges/grvt');
function ast(file){return ts.createSourceFile(file,fs.readFileSync(path.join(source,file),'utf8'),ts.ScriptTarget.Latest,true);}
const tree=ast('grvt.ws-public.ts'),cls=tree.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text==='GrvtPublicWebSocket');
const methods=['mapIntervalToGrvt','subscribeToKline','unsubscribeFromKline','subscribeToOrderbook','unsubscribeFromOrderbook','subscribeToTicker','unsubscribeFromTicker','cancelTickerBatches','ensureTickerSubscription'];
const members=methods.map(name=>{const n=cls.members.find(n=>n.name?.getText(tree)===name);assert(n,'actual '+name);return n.getText(tree);});
const types=ast('grvt.types.ts'),interval=types.statements.find(n=>ts.isEnumDeclaration(n)&&n.name.text==='GrvtCandlestickInterval');
const code=ts.transpileModule([interval.getText(types),'export class Venue {',...members,'}'].join('\n'),{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022},reportDiagnostics:true});assert.equal(code.diagnostics.length,0);
let observe=()=>{},clear=()=>{};
const context={exports:{},JSON:{stringify(value){observe(value);return JSON.stringify(value);}},clearTimeout(timer){clear(timer);}};
vm.runInNewContext(code.outputText,context);const Venue=context.exports.Venue;
function tag(v){if(typeof v==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(v);return {bits:b.toString('hex'),nan:Number.isNaN(v)};}if(v===undefined)return {undefined:true};if(typeof v==='function')return {function:true,text:String(v),arity:v.length};if(Array.isArray(v))return v.map(tag);if(v&&typeof v==='object')return Object.fromEntries(Object.entries(v).map(([k,v])=>[k,tag(v)]));return v;}
function make(){const v=new Venue();v.requestId=1;v.subscriptions=new Map();v.tickerSubscriptions=new Map();v.tickerBatchTimers=new Set([101,202]);v.tickerBatchGeneration=0;v.isDisposed=false;v.isConnected=true;return v;}
function snapshot(v){return tag({requestId:v.requestId,subscriptions:[...v.subscriptions],tickers:[...v.tickerSubscriptions],timers:[...v.tickerBatchTimers],generation:v.tickerBatchGeneration});}
function capture(v,name,action){const effects=[],requests=[],wires=[];let nested=false;observe=request=>requests.push(tag(request));clear=timer=>{effects.push(['clear',timer]);if(name==='clear-error')throw Error('clear-fail');if(name==='reentrant-add'&&timer===101)v.tickerBatchTimers.add(303);if(name==='reentrant-remove'&&timer===101)v.tickerBatchTimers.delete(202);if(name==='nested-cancel-add'&&timer===101&&!nested){nested=true;v.cancelTickerBatches();v.tickerBatchTimers.add(303);}};v.send=wire=>{wires.push(wire);effects.push(['send',snapshot(v)]);if(name==='send-delete')v.subscriptions.clear();if(name==='send-error')throw Error('send-fail');};let result,error;try{result=action();}catch(e){error={name:e.name,message:e.message};}return {name,result:tag(result),error:error??null,requests,wires,effects,state:snapshot(v)};}
const proto=['constructor','__defineGetter__','__defineSetter__','hasOwnProperty','__lookupGetter__','__lookupSetter__','isPrototypeOf','propertyIsEnumerable','toString','valueOf','__proto__','toLocaleString'];
const intervalNames=['1m','3m','5m','15m','30m','1h','2h','4h','6h','8h','12h','1d','3d','5d','1w','2w','3w','4w','CI_5_M','unknown','',...proto];
const mapRows=intervalNames.map(input=>{const value=make().mapIntervalToGrvt(input);return {input,value:tag(value),text:String(value),kind:typeof value};});
const subRows=[];
for(const method of ['subscribeToKline','subscribeToOrderbook','subscribeToTicker'])for(const name of ['default','explicit','send-error','send-delete','large-id','negative-zero','nan-id','infinity-id',...(method==='subscribeToKline'?['inherited']:[])]){
 const v=make();if(name==='large-id')v.requestId=9007199254740992;if(name==='negative-zero')v.requestId=-0;if(name==='nan-id')v.requestId=NaN;if(name==='infinity-id')v.requestId=Infinity;
 const row=capture(v,name,()=>method==='subscribeToKline'?v[method]('BTC-ß',name==='inherited'?'constructor':'8h',...(name==='explicit'?['MARK']:[])):method==='subscribeToOrderbook'?v[method]('BTC-ß',...(name==='explicit'?[-0,Infinity]:[])):v[method]('BTC-ß',...(name==='explicit'?[25.5,'v1.ticker.d']:[])));
 subRows.push({method,...row});
}
const unsubRows=[];
for(const method of ['unsubscribeFromKline','unsubscribeFromOrderbook','unsubscribeFromTicker'])for(const name of ['default','missing','send-error','clear-error','nan-key','negative-zero','send-delete']){
 const v=make();let id=name==='nan-key'?NaN:name==='negative-zero'?-0:1;
 v.subscriptions.set(id,{jsonrpc:'2.0',id,method:'subscribe',params:{stream:'v1.ticker.d',selectors:['SEED@500']}});v.tickerSubscriptions.set('FIRST',id);v.tickerSubscriptions.set('SECOND',id);v.requestId=9;
 unsubRows.push({method,input:tag(id),...capture(v,name,()=>v[method](name==='missing'?17:id))});
}
const cancelRows=['default','clear-error','reentrant-add','reentrant-remove','nested-cancel-add','generation-large'].map(name=>{const v=make();if(name==='generation-large')v.tickerBatchGeneration=9007199254740992;return capture(v,name,()=>v.cancelTickerBatches());});
const ensureRows=['new','existing','disposed','disconnected','send-error','send-delete','disposed-short-circuit'].map(name=>{
 const v=make();if(name==='existing')v.tickerSubscriptions.set('BTC',77);v.isDisposed=name.startsWith('disposed');v.isConnected=name!=='disconnected';
 if(name==='disposed-short-circuit')Object.defineProperty(v,'isConnected',{get(){throw Error('guard-fail');}});
 return capture(v,name,()=>v.ensureTickerSubscription('BTC'));
});
assert.equal(mapRows.length,33);assert.equal(subRows.length,25);assert.equal(unsubRows.length,21);assert.equal(cancelRows.length,6);assert.equal(ensureRows.length,7);
const inheritedCall=make().mapIntervalToGrvt('hasOwnProperty').call({owned:1},'owned');assert.equal(inheritedCall,true);
process.stdout.write(JSON.stringify({mapRows,subRows,unsubRows,cancelRows,ensureRows,inheritedCall}));
