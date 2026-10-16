const fs=require('node:fs'),path=require('node:path');const {ts,root}=require('./source.cjs');
let source=fs.readFileSync(path.join(root,'packages/safe-cex/src/exchanges/asterdex/asterdex.ws-private.ts'),'utf8');
if(process.env.ASTERDEX_ACCOUNT_UPDATE_CONTROL==='emit-first') {
 const start=source.indexOf('  private handleAccountUpdate(data: any): void {');
 const end=source.indexOf('\n  private handleAccountPositionUpdate',start);
 if(start<0||end<0) throw Error('handleAccountUpdate source control target missing');
 let method=source.slice(start,end);
 const emit="    this.parent.emitter.emit('account', data);\n";
 if(method.split(emit).length!==2) throw Error('handleAccountUpdate emit control target missing');
 method=method.replace(emit,'').replace('  private handleAccountUpdate(data: any): void {\n','  private handleAccountUpdate(data: any): void {\n'+emit);
 source=source.slice(0,start)+method+source.slice(end);
}
if(process.env.ASTERDEX_FILL_CONTROL==='swap') {
 const original='        filled,\n        remaining: Math.max(amount - filled, 0),';
 if(!source.includes(original)) throw Error('mirrorCumulativeOrderFill source control target missing');
 source=source.replace(original,'        filled: amount,\n        remaining: Math.max(amount - filled, 0),');
}
if(process.env.ASTERDEX_EXECUTION_STATUS_CONTROL==='filled-open') {
 const original='FILLED: OrderStatus.Closed';
 if(!source.includes(original)) throw Error('handleExecutionReport source control target missing');
 source=source.replace(original,'FILLED: OrderStatus.Open');
}
const ast=ts.createSourceFile('private.ts',source,ts.ScriptTarget.Latest,true),printer=ts.createPrinter();
const cls=ast.statements.find(ts.isClassDeclaration),names=['parseFiniteNumber','getOrderUpdateTime','parseAssetBalanceUpdates','getPositionSide','getAccountUpdateUpnlDeltas','mirrorCumulativeOrderFill','handleExecutionReport','handleAccountUpdate','ACCOUNT_CURRENCY'];
const members=names.map(name=>{const n=cls.members.find(n=>n.name?.getText(ast)===name);if(!n)throw Error('Missing '+name);return printer.printNode(ts.EmitHint.Unspecified,n,ast)}).join('\n');
const shared=ts.createSourceFile('types.ts',fs.readFileSync(path.join(root,'packages/safe-cex/src/types.ts'),'utf8'),ts.ScriptTarget.Latest,true);const enums=shared.statements.filter(ts.isEnumDeclaration).map(n=>printer.printNode(ts.EmitHint.Unspecified,n,shared)).join('\n');
let clockCalls=0;const clock={getCurrentTimeMs:()=>{clockCalls++;return 1234}};const out={};new Function('exports','virtualClock',ts.transpileModule(enums+'\nexport class AsterDEXPrivateWebsocket{'+members+'}',{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText)(out,clock);const o=new out.AsterDEXPrivateWebsocket();
const encode=value=>JSON.parse(JSON.stringify(value??null,(_,v)=>typeof v==='number'&&(!Number.isFinite(v)||Object.is(v,-0))?{number:Object.is(v,-0)?'-0':String(v)}:v));
const result=fn=>{try{return {value:encode(fn())}}catch(e){return {error:{name:e.name,message:e.message}}}};
const values=[undefined,null,false,true,0,-1,0.5,1,'',' ','\ufeff','\u0085','0x10','12abc','1e309','-0',[],[1],{}, {toString:null}];
const finite=values.map(input=>({input,output:result(()=>o.parseFiniteNumber(input))}));
const times=[];for(const T of values)for(const E of [undefined,0,'2',null]){const input={...(T===undefined?{}:{T}),...(E===undefined?{}:{E})};clockCalls=0;const output=result(()=>o.getOrderUpdateTime(input));times.push({input,output,clockCalls});}times.push({input:null,output:result(()=>o.getOrderUpdateTime(null)),clockCalls:0});
const balances=[];const fieldsList=[{free:'f',used:'l'},{total:'wb',free:'cw'},{total:'',free:'f',used:'l'},{free:'f'}];for(const fields of fieldsList)for(const input of [undefined,null,[],{},[null],[{a:'USDT',f:'2',l:'1',wb:'3',cw:'2'}],[{a:'\ufeffUSDT ',f:'-1',l:'1',wb:'3',cw:'2'}],[{a:'\u0085',f:'2',l:'1',wb:'3',cw:'2'}],[{a:'USDT',f:'2',l:'-1',wb:'1',cw:'2'}],[{a:'USDT',f:'1e308',l:'1e308',wb:'1e308',cw:'-1e308'}],[{a:'USDT',f:'2',l:'1',wb:'3',cw:'2'},null],[{a:'',f:2,l:0}],[{a:'USDT',f:'0x10',l:'12abc',wb:'3',cw:'2'}]])balances.push({input,fields,output:result(()=>o.parseAssetBalanceUpdates(input,fields))});
const sides=[];for(const input of [null,{}, {ps:'BOTH'},{ps:'LONG'},{ps:'SHORT'},{ps:''},{ps:'long'}])for(const amount of [-1,0,1,NaN])sides.push({input,amount:encode(amount),output:result(()=>o.getPositionSide(input,amount))});
const upnl=[];
const inputs=[undefined,null,{},[],[null],[{s:'',pa:1,up:1}],[{s:'BTCUSDT',pa:'1',up:'10',ps:'BOTH'}],[{s:'BTCUSDT',pa:'-1',up:'10',ps:'BOTH'}],[{s:'BTCUSDT',pa:1,up:10,ps:'LONG'},{s:'ETHUSDT',pa:1,up:2,ps:'LONG'},{s:'BTCUSDT',pa:1,up:3,ps:'LONG'}],[{s:'X2',pa:0,up:-0}],[{s:'ETHBTC',pa:1,up:2}],[{s:'BTCUSDT',pa:1,up:10},null],[{s:'BTCUSDT',pa:'oops',up:10}],[{s:'BTCUSDT',pa:1,up:'Infinity'}],[{s:'BTCUSDT',pa:1,up:1e308},{s:'BTCUSDT',pa:1,up:1e308},{s:'BTCUSDT',pa:1,up:-1e308}]];
for(const keys of [[],['USDT','DT'],['DT','USDT'],['','USDT'],['BTC','USDT'],['2','1'],['__proto__']])for(const previous of [[],[{symbol:'BTCUSDT',side:'long',unrealizedPnl:4},{symbol:'BTCUSDT',side:'long',unrealizedPnl:8}],[{symbol:'BTCUSDT',side:'short',unrealizedPnl:2}],[{symbol:'BTCUSDT',side:'short',unrealizedPnl:Infinity}]])for(const input of inputs){o.parent={store:{positions:previous,balance:{currencies:Object.fromEntries(keys.map(k=>[k,{}]))}}};upnl.push({input,keys,previous:encode(previous),output:result(()=>{const x=o.getAccountUpdateUpnlDeltas(input);return x===undefined?undefined:[...x]})});}
// Execute the production constructor/update/getters: a hand-built store can hide writes.
const cloneExports={};new Function('exports',ts.transpileModule(fs.readFileSync(path.join(root,'packages/safe-cex/src/utils/clone.ts'),'utf8'),{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText)(cloneExports);
const storeAst=ts.createSourceFile('store.ts',fs.readFileSync(path.join(root,'packages/safe-cex/src/store/store.base.ts'),'utf8'),ts.ScriptTarget.Latest,true);
const storeSource=storeAst.statements.filter(n=>!ts.isImportDeclaration(n)).map(n=>printer.printNode(ts.EmitHint.Unspecified,n,storeAst)).join('\n');
const storeExports={};new Function('exports','clone','virtualClock',ts.transpileModule(enums+'\n'+storeSource,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText)(storeExports,cloneExports.clone,clock);
const storeUpnl=[];
for(const keys of [['USDT','DT'],['DT','USDT'],['10','2','USDT','DT'],['USDT','DT','2','1']]) {
 const leaf=()=>({used:0,free:0,total:0,upnl:0});const currencies={};for(const key of keys)currencies[key]=leaf();
 const balance={...leaf(),currencies};const balanceJson=JSON.stringify(balance);
 const store=new storeExports.DefaultStore();store.update({balance:JSON.parse(balanceJson)});o.parent={store};
 const input=[{s:'BTCUSDT',pa:'1',up:'10',ps:'BOTH'}];
 storeUpnl.push({balanceJson,input,keys:Object.keys(store.balance.currencies),output:result(()=>[...o.getAccountUpdateUpnlDeltas(input)])});
}
const fills=[];
for(const input of [
 null,
 {i:17,X:'PARTIALLY_FILLED',z:'2.5',q:'10',T:100},
 {i:'order-2',X:'FILLED',z:'12',q:'10',T:0,E:200},
 {i:0,X:'PARTIALLY_FILLED',z:'2',q:'5'},
 {i:3,X:'FILLED',z:'0',q:'0'},
 {X:'FILLED',z:'1',q:'2'},
 {i:null,X:'FILLED',z:'1',q:'2'},
 {i:null,z:{toString:false},q:'1'},
 {z:'1',q:{toString:false}},
 {i:{toString:null},X:'FILLED',z:'1',q:'2'},
 {i:4,X:'PARTIALLY_FILLED',z:'bad',q:'2'},
 {i:5,X:'PARTIALLY_FILLED',z:'1',q:'Infinity'},
 {i:6,X:'PARTIALLY_FILLED',z:0,q:5},
 {i:7,X:'PARTIALLY_FILLED',z:'-0',q:'2',T:-1,E:300},
]) {
 const writes=[];clockCalls=0;o.parent={store:{updateOrder:(order,patch)=>writes.push({order,patch})}};
 const output=result(()=>o.mirrorCumulativeOrderFill(input));
 fills.push({input,output,writes:encode(writes),clockCalls});
}
const executionReports=[];
for(const input of [
 {i:'order-1',X:'NEW',z:'2.5',q:'10'},
 {i:'order-1',X:'FILLED',z:'9',q:'10'},
 {i:'order-1',X:'REJECTED',z:'0',q:'3'},
 {i:'order-1',X:'UNKNOWN',z:'1',q:'4'},
 {i:'order-1',X:'constructor',z:'1',q:'4'},
 {i:'order-1',X:'__proto__',z:'1',q:'4'},
 {i:'missing',X:'FILLED',z:'1',q:'4'},
 {i:0,X:'FILLED',z:'1',q:'4'},
 null,
]) {
 const writes=[];let schedules=0,events=0;o.scheduleOrderReconcile=()=>{schedules++};
 o.parent={store:{orders:[{id:'order-1'}],updateOrder:(order,patch)=>writes.push({orderId:order.id,statusType:typeof patch.status,statusString:typeof patch.status==='function'||typeof patch.status==='object'?String(patch.status):String(patch.status),patch})},emitter:{emit:()=>events++}};
 const output=result(()=>o.handleExecutionReport(input));
 executionReports.push({input,output,writes:encode(writes.map(({orderId,statusType,statusString,patch})=>({orderId,statusType,statusString,filled:patch.filled,remaining:patch.remaining}))),schedules,events});
}
const accountUpdates=[];
for(const input of [null,false,0,'',{}, {e:'outboundAccountPosition'}, {B:false}, {B:0}, {B:''}, {B:[]}, {B:{}}, {B:[{a:'USDT',f:'10',l:'2'}]}, {B:[{a:'USDT',f:'2',l:'1'},{a:'DT',f:'4.5',l:'0.5'}]}, {B:[{a:'USDT',f:'2',l:'-1'}]}, {B:[{a:'USDT',f:'1e308',l:'1e308'}]}]) {
 const effects=[];
 o.applyBalanceUpdates=balances=>effects.push(['balances',encode(balances)]);
 o.parent={emitter:{emit:(name,value)=>effects.push(['emit',name,encode(value)])}};
 const output=result(()=>o.handleAccountUpdate(input));
 accountUpdates.push({input:encode(input),output,effects});
}
process.stdout.write(JSON.stringify({storeUpnl,finite,times,balances,sides,upnl,fills,executionReports,accountUpdates}));
