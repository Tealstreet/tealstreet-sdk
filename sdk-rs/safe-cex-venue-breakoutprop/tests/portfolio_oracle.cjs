const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
assert(process.env.SAFE_CEX_TYPESCRIPT_PATH,'SAFE_CEX_TYPESCRIPT_PATH required');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH),root=path.resolve(__dirname,'../../..');
const read=p=>fs.readFileSync(path.join(root,'packages/safe-cex/src',p),'utf8');
const parse=s=>ts.createSourceFile('source.ts',s,ts.ScriptTarget.Latest,true);
function execute(source,imports={}){const module={exports:{}};vm.runInNewContext(ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022,esModuleInterop:true}}).outputText,{exports:module.exports,module,require:k=>{assert(k in imports,'unexpected import '+k);return imports[k];}});return module.exports;}
const enums=execute(parse(read('types.ts')).statements.filter(ts.isEnumDeclaration).map(n=>n.getText()).join('\n'));
const typesSource=read('exchanges/breakoutprop/breakoutprop.types.ts');
const constants=execute(typesSource,{'../../types':enums});
const ids=execute(read('utils/createPositionId.ts'),{'../types':enums});
const unused=()=>{throw Error('unexpected dependency execution');};
class Base {constructor(name,options,store){this.options=options;this.store=store;this.errors=[];}emitAndLogError(error){this.errors.push(error);}}
const {BreakoutpropExchange}=execute(read('exchanges/breakoutprop/breakoutprop.exchange.ts'),{
 'axios-rate-limit':v=>v,'lodash/chunk':unused,'../../types':enums,'../../utils/createPositionId':ids,
 '../../utils/inverseObj':{inverseObj:unused},'../../utils/loop':{loop:unused},'../../utils/omitUndefined':{omitUndefined:unused},
 '../../utils/safe-math':{adjust:unused},'../../utils/uuid':{uuid:unused},'../base':{BaseExchange:Base},
 './breakoutprop.api':{createAPI:()=>({})},'./breakoutprop.types':constants,'./breakoutprop.ws-public':{BreakoutpropPublicWebsocket:class{}},
});
let counter=0;
function fixture(node){
 if(ts.isTypeLiteralNode(node))return Object.fromEntries(node.members.map(m=>{assert(ts.isPropertySignature(m)&&!m.questionToken,'fixture field syntax');return [m.name.text,fixture(m.type)];}));
 if(ts.isArrayTypeNode(node))return [fixture(node.elementType)];
 counter++;
 switch(node.kind){case ts.SyntaxKind.StringKeyword:return 'field-'+counter;case ts.SyntaxKind.NumberKeyword:return counter+.125;case ts.SyntaxKind.BooleanKeyword:return counter%2===0;case ts.SyntaxKind.AnyKeyword:return {retained:-0};default:throw Error('unsupported fixture syntax');}
}
const portfolioType=parse(typesSource).statements.find(n=>ts.isTypeAliasDeclaration(n)&&n.name.text==='PORTFOLIO_RESPONSE');
function data(){const d=fixture(portfolioType.type),p=d.portfolios[0];
 Object.assign(p.balances[0],{currency:'USD',value:131.25});
 Object.assign(p.positions[0],{symbol:'BTCUSD',side:'BUY',quantity:2.5,quantityNotional:211,openPrice:37,marginRate:.2,lastUpdateTime:'2024-12-11T06:00:49.087Z'});
 const second=structuredClone(p.positions[0]);Object.assign(second,{symbol:'ETHUSD',side:'SELL',quantity:3,quantityNotional:-317,openPrice:53,marginRate:.4,lastUpdateTime:'2024-12-12T07:01:50.123Z'});p.positions.push(second);
 const o=p.orders[0];Object.assign(o,{orderCode:'id-A',instrument:'BTCUSD',status:'WORKING',type:'LIMIT',transactionTime:'2024-12-13T08:02:51.456Z'});
 Object.assign(o.legs[0],{side:'BUY',price:17,quantity:11,filledQuantity:3,remainingQuantity:7.5,positionEffect:'CLOSE'});
 p.opaqueExtra={negativeZero:-0,overflow:Infinity,unicode:'é😀'};o.opaqueOrder={value:-Infinity};p.positions[0].opaquePosition={value:-0};return d;
}
const market=symbol=>({id:symbol,symbol,base:symbol.replace('USD',''),quote:'USD',active:true,precision:{amount:.1,price:.01},limits:{amount:{min:1,max:100},leverage:{min:1,max:10}}});
const ticker=(symbol,mark,last)=>({id:symbol,symbol,bid:29,ask:31,last,mark,index:35,percentage:1,openInterest:2,fundingRate:3,fundingTime:4,volume:5,quoteVolume:6});
function store(){const tickers=[ticker('BTCUSD',43,47),ticker('ETHUSD',41,59)];return {balance:{total:997,free:893,used:71,upnl:23},positions:[],orders:[{id:'cached-order',symbol:'OLD',type:'market',side:'sell',price:101,amount:13,filled:2,remaining:9,reduceOnly:false,status:'open',lastUpdate:555}],markets:['BTCUSD','ETHUSD','MISSING'].map(market),tickers,tickersBySymbol:Object.fromEntries(tickers.map(t=>[t.symbol,t]))};}
function encode(v){if(typeof v==='number')return Number.isNaN(v)?(()=>{throw Error('NaN input is not JSON');})():v===Infinity?'1e400':v===-Infinity?'-1e400':Object.is(v,-0)?'-0':String(v);if(Array.isArray(v))return '['+v.map(encode).join(',')+']';if(v&&typeof v==='object')return '{'+Object.entries(v).map(([k,x])=>JSON.stringify(k)+':'+encode(x)).join(',')+'}';return JSON.stringify(v);}
function observe(v){if(v===undefined)return {$undefined:true};if(typeof v==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(v);return {number:Number.isNaN(v)?'7ff8000000000000':b.toString('hex')};}if(typeof v==='function')return {$function:String(v)};if(Array.isArray(v))return v.map(observe);if(v&&typeof v==='object')return Object.fromEntries(Object.entries(v).map(([k,x])=>[k,observe(x)]));return v;}
const specs=[
 ['distinct',()=>{}],
 ['missing-unused-account',d=>{delete d.portfolios[0].account;}],
 ['last-fallback',(d,s)=>{s.tickers[0].mark=0;s.tickers[1].mark=-0;}],
 ['entry-fallback',(d,s)=>{s.tickers=[];}],
 ['nan-mark',(d,s)=>{s.nanMark=true;}],
 ['zero-tickers',(d,s)=>{s.tickers.forEach(t=>{t.mark=0;t.last=-0;});}],
 ['empty-portfolios',d=>{d.portfolios=[];}],
 ['empty-positions',d=>{d.portfolios[0].positions=[];}],
 ['no-usd',d=>{d.portfolios[0].balances[0].currency='EUR';}],
 ['empty-symbols',()=>{},[]],
 ['duplicate-missing',()=>{},['BTCUSD','MISSING','MISSING']],
 ['short-stop',d=>{d.portfolios[0].orders[0].type='STOP';Object.assign(d.portfolios[0].orders[0].legs[0],{side:'SELL',positionEffect:'OPEN'});}],
 ['market-order',d=>{d.portfolios[0].orders[0].type='MARKET';}],
 ['unknown-before-empty-leg',d=>{const o=d.portfolios[0].orders[0];o.status='UNKNOWN';const second=structuredClone(o);second.legs=[];d.portfolios[0].orders.push(second);}],
 ['empty-leg',d=>{d.portfolios[0].orders[0].legs=[];}],
 ['invalid-date',d=>{d.portfolios[0].positions[0].lastUpdateTime='invalid';d.portfolios[0].orders[0].transactionTime='invalid';}],
 ['utc-local-date',d=>{d.portfolios[0].positions[0].lastUpdateTime='2024-01-02T03:04:05';}],
 ['zero-margin',d=>{d.portfolios[0].positions[0].marginRate=0;}],
 ['nonfinite',(d,s)=>{d.portfolios[0].positions[0].openPrice=Infinity;s.tickers[0].mark=Infinity;}],
 ['signed-zero',d=>{Object.assign(d.portfolios[0].positions[0],{quantity:-0,quantityNotional:-0,marginRate:-0,openPrice:-0});Object.assign(d.portfolios[0].orders[0].legs[0],{price:-0,quantity:-0,filledQuantity:-0,remainingQuantity:-0});}],
 ['unknown-position-side',d=>{d.portfolios[0].positions[0].side='UNKNOWN';}],
 ['unknown-order-status',d=>{d.portfolios[0].orders[0].status='UNKNOWN';}],
 ['unknown-order-type',d=>{d.portfolios[0].orders[0].type='UNKNOWN';}],
 ['unknown-order-side',d=>{d.portfolios[0].orders[0].legs[0].side='UNKNOWN';}],
];
const methods=['formatBalance','formatOpenPositions','formatPositions','formatOrders'];
for(const key of ['constructor','toString','__proto__'])specs.push(['inherited-before-empty-leg-'+key,d=>{const o=d.portfolios[0].orders[0];o.status=key;const second=structuredClone(o);second.legs=[];d.portfolios[0].orders.push(second);}]);
const inherited=[];
for(const key of ['constructor','toString','__proto__'])for(const field of ['position-side','status','type','side']){
 const input=data();
 if(field==='position-side')input.portfolios[0].positions[0].side=key;
 else if(field==='side')input.portfolios[0].orders[0].legs[0].side=key;
 else input.portfolios[0].orders[0][field]=key;
 const ex=new BreakoutpropExchange('breakoutprop',{},store());
 const native=field==='position-side'?ex.formatOpenPositions(input)[0].side:ex.formatOrders(input)[0][field];
 const beforeSerialization={type:typeof native,truthy:!!native};
 const methodsSucceed=methods.filter(method=>field==='position-side'?method!=='formatOrders':method==='formatOrders').map(method=>{ex[method](input);return method;});
 inherited.push({key,field,raw:encode(input),beforeSerialization,methodsSucceed});
}
const cases=specs.map(([name,change,symbols])=>{const d=data(),s=store();change(d,s);s.tickersBySymbol=Object.fromEntries(s.tickers.map(t=>[t.symbol,t]));const raw=encode(d),storeRaw=encode(s),expected={};for(const method of methods){const nativeStore=JSON.parse(storeRaw);if(s.nanMark)nativeStore.tickersBySymbol.BTCUSD.mark=NaN;const ex=new BreakoutpropExchange('breakoutprop',{},nativeStore);try{expected[method]={ok:observe(ex[method](JSON.parse(raw),symbols)),errors:ex.errors};}catch(e){expected[method]={error:e.message,errors:ex.errors};}}return {name,raw,storeRaw,symbols:symbols??null,expected};});
const aliases=methods.map(method=>{const input=data(),ex=new BreakoutpropExchange('breakoutprop',{},store());const result=ex[method](input);
 if(method==='formatBalance')result.info.portfolios[0].positions[0].openPrice=999;
 else if(method==='formatOrders')result[0].info.orderCode='changed-through-result';
 else result[0].info.openPrice=999;
 const changed=method==='formatOrders'?input.portfolios[0].orders[0].orderCode:input.portfolios[0].positions[0].openPrice;
 return {method,changed};
});
const fallbackAliases=['formatBalance','formatOrders'].map(method=>{
 const input=data(),s=store(),ex=new BreakoutpropExchange('breakoutprop',{},s);
 if(method==='formatBalance'){input.portfolios=[];ex[method](input).free=123;return {method,observed:s.balance.free};}
 input.portfolios[0].orders[0].legs=[];ex[method](input)[0].id='changed-cache';return {method,observed:s.orders[0].id};
});
const surrogateInput=data();surrogateInput.portfolios[0].positions[0].symbol='\ud800';surrogateInput.portfolios[0].orders[0].instrument='\ud800';
const surrogateRaw=encode(surrogateInput);
const surrogates=methods.map(method=>{const input=JSON.parse(surrogateRaw),ex=new BreakoutpropExchange('breakoutprop',{},store());const result=ex[method](input,['\ud800']);
 const text=method==='formatBalance'?result.info.portfolios[0].positions[0].symbol:result[0].symbol;
 return {method,raw:surrogateRaw,codeUnit:text.charCodeAt(0)};
});
const corpus={inherited,fallbackAliases,aliases,surrogates,runtime:{node:process.version,major:process.versions.node.split('.')[0],arch:process.arch,v8:process.versions.v8},cases};
if(require.main===module)console.log(JSON.stringify(corpus));
else module.exports={corpus,BreakoutpropExchange,store,encode,observe};
