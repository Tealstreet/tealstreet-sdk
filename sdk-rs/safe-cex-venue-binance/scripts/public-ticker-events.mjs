import assert from 'node:assert/strict';import fs from 'node:fs';import vm from 'node:vm';import {createRequire} from 'node:module';
const require=createRequire(import.meta.url),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const lanes=[['futures_ticker','binance','BinanceMarketWebsocket','handleTickerStreamEvents'],['futures_mark','binance','BinanceMarketWebsocket','handleMarkPriceStreamEvents'],['futures_book','binance','BinancePublicWebsocket','handleBookTickersStreamEvents'],['pm_ticker','binancepm','BinancePMMarketWebsocket','handleTickerStreamEvents'],['pm_mark','binancepm','BinancePMMarketWebsocket','handleMarkPriceStreamEvents'],['pm_book','binancepm','BinancePMPublicWebsocket','handleBookTickersStreamEvents'],['spot_ticker','binancespot','BinancePublicWebsocket','handleTickerStreamEvents'],['spot_book','binancespot','BinancePublicWebsocket','handleBookTickersStreamEvents']];
const cases=['normal','empty','unknown','missing-fields','alias-case','numeric-values','partial-numeric','nan-values','array-values','coercion-fail','symbol-null','symbol-array','symbol-object','symbol-coercion-fail','event-null','event-undefined','primitive-event','first-update-fail','second-update-fail','capture-fail','update-getter-fail','getter-mutates-event','swap-store','mutate-dictionary','append-event','replace-event','truncate-events','false-ticker'];
const fields=['last','mark','index','percentage','volume','quoteVolume','fundingRate','fundingTime','bid','ask'];
const rawKeys=['c','P','v','q','p','i','r','T','b','a'];
const first=()=>({s:'BTCUSDT',c:'12.3',P:'2.5',v:'3.6',q:'44.28',p:'13.1',i:'14.2',r:'.001',T:'1700123456789',b:'12.2',a:'12.4'}),second=()=>({s:'ETHUSDT',c:'22.3',P:'5.5',v:'7.6',q:'77.28',p:'23.1',i:'24.2',r:'.002',T:'1701123456789',b:'22.2',a:'22.4'});
const encode=n=>Number.isNaN(n)?'nan':n===Infinity?'infinity':n===-Infinity?'-infinity':Object.is(n,-0)?'-zero':n;
const vExports={};vm.runInNewContext(ts.transpileModule(fs.readFileSync(new URL('../../../packages/safe-cex/src/utils/get-key.ts',import.meta.url),'utf8'),{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022,esModuleInterop:true}}).outputText,{exports:vExports,require});
const baseFile=new URL('../../../packages/safe-cex/src/exchanges/base.ws.ts',import.meta.url),baseAst=ts.createSourceFile(baseFile.pathname,fs.readFileSync(baseFile,'utf8'),ts.ScriptTarget.Latest,true),baseDecl=baseAst.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text==='BaseWebSocket'),storeGetter=baseDecl.members.find(n=>ts.isGetAccessor(n)&&n.name.getText(baseAst)==='store');assert.ok(storeGetter);
const results=[];
for(const[lane,venue,klass,member]of lanes){
 const file=new URL(`../../../packages/safe-cex/src/exchanges/binance/${venue}.ws-public.ts`,import.meta.url),ast=ts.createSourceFile(file.pathname,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true),decl=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text===klass),method=decl.members.find(n=>n.name?.getText(ast)===member);assert.ok(method);
 const exports={};vm.runInNewContext(ts.transpileModule(`exports.Subject=class {${storeGetter.getText(baseAst)}\n${method.getText(ast)}}`,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,{exports,v:vExports.v,parseFloat});
 for(const name of cases){
  const instance=new exports.Subject(),trace=[],updates=[],error=RangeError('ticker fixture'),a=first(),b=second(),unknown={s:'UNKNOWN',c:{toString:0},p:{toString:0},b:{toString:0}},btc={id:1,...Object.fromEntries(fields.map((key,i)=>[key,90+i]))},eth={id:2,...Object.fromEntries(fields.map((key,i)=>[key,190+i]))};
  const dictionary={BTCUSDT:btc,ETHUSDT:eth};let events=[a,unknown,b];
  if(name==='empty')events=[];if(name==='unknown')events=[unknown];
  if(['missing-fields','alias-case','numeric-values','partial-numeric','nan-values','array-values','coercion-fail','symbol-null','symbol-array','symbol-object','symbol-coercion-fail','event-null','event-undefined','primitive-event','getter-mutates-event','false-ticker'].includes(name))events=[a];
  if(name==='missing-fields')events=[{s:'BTCUSDT'}];
  if(name==='alias-case'){delete a.P;delete a.T;a.t='1700123000000';}
  if(name==='numeric-values')for(const key of rawKeys)a[key]=parseFloat(a[key]);
  if(name==='partial-numeric')for(const key of rawKeys)a[key]+='tail';
  if(name==='nan-values')for(const key of rawKeys)a[key]={};
  if(name==='array-values')for(const key of rawKeys)a[key]=['8.5tail',null];
  if(name==='coercion-fail')for(const key of rawKeys)a[key]={toString:0};
  if(name==='symbol-null'){a.s=null;dictionary.null=btc;}if(name==='symbol-array'){a.s=[1,2];dictionary['1,2']=btc;}if(name==='symbol-object'){a.s={};dictionary['[object Object]']=btc;}if(name==='symbol-coercion-fail')a.s={toString:0};
  if(name==='event-null')events=[null];if(name==='event-undefined')events=[undefined];if(name==='primitive-event'){events=[7];dictionary.undefined=btc;}if(name==='false-ticker')dictionary.BTCUSDT=0;
  const tracked=new Proxy(dictionary,{get(target,key,receiver){trace.push(['lookup',String(key)]);return Reflect.get(target,key,receiver);}});
  let updateCalls=0;
  const makeStore=id=>({id,get tickersBySymbol(){trace.push(['capture',id]);if(name==='capture-fail')throw error;return id===10?tracked:{};},get updateTicker(){trace.push(['get-update',id]);if(name==='update-getter-fail')throw error;if(name==='getter-mutates-event')for(const key of rawKeys)a[key]='31.25';return function(ticker,patch){updateCalls++;trace.push(['update',this.id,ticker.id]);assert.equal(ticker,ticker.id===1?btc:eth);updates.push({receiver:this.id,ticker:ticker.id,fields:Object.entries(patch).map(([key,value])=>[key,encode(value)]),identity:true});Object.assign(ticker,patch);if(name==='swap-store'&&updateCalls===1)instance.parent.store=secondary;if(name==='mutate-dictionary'&&updateCalls===1)delete dictionary.ETHUSDT;if(name==='append-event'&&updateCalls===1)events.push(second());if(name==='replace-event'&&updateCalls===1)events[2]={...b,s:'UNKNOWN'};if(name==='truncate-events'&&updateCalls===1)events.length=1;if((name==='first-update-fail'&&updateCalls===1)||(name==='second-update-fail'&&updateCalls===2))throw error;};}});
  const primary=makeStore(10),secondary=makeStore(20);instance.parent={store:primary};
  let failure=null,errorName=null,errorMessage=null;
  try{instance[member](events);}catch(e){failure=e===error?'original':'generated';errorName=e.name;errorMessage=e.message;}
  results.push({lane,name,trace,updates,currentStore:instance.parent.store.id,length:events.length,failure,errorName,errorMessage,btc:fields.map(key=>[key,encode(btc[key])]),eth:fields.map(key=>[key,encode(eth[key])])});
 }
}
assert.equal(results.length,224,'independently authored floor 8*28');process.stdout.write(JSON.stringify(results));
