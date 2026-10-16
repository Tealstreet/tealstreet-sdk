'use strict';
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict'),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const file=path.resolve(__dirname,'../../../packages/safe-cex/src/exchanges/lighter/lighter.exchange.ts'),text=fs.readFileSync(file,'utf8'),ast=ts.createSourceFile(file,text,ts.ScriptTarget.Latest,true);assert.equal(ast.parseDiagnostics.length,0);
const names=['rememberPlacedOrderHint','readPlacedOrderHint','rememberRecentAccountSnapshot','invalidateAccountSnapshotCache','getCachedAccountTier','shouldSkipOrderReconciliationFetch','getCurrentMarginModeForSymbol','getCurrentLeverageForSymbol'];
const cls=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text==='LighterExchange');
const members=names.map(name=>{const a=cls.members.filter(n=>n.name?.getText(ast)===name);assert.equal(a.length,1);assert.ok(ts.isArrowFunction(a[0].initializer));return a[0].getText(ast)});
const cooldown=ast.statements.flatMap(n=>ts.isVariableStatement(n)?Array.from(n.declarationList.declarations):[]).find(n=>n.name.getText(ast)==='LIGHTER_ORDER_RECONCILIATION_COOLDOWN_MS');assert.equal(cooldown.initializer.getText(ast),'3_000');
const compiled=ts.transpileModule(`class Subject {${members.join('\n')}} exports.Subject=Subject;`,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS},reportDiagnostics:true});assert.equal(compiled.diagnostics.length,0);
const u=s=>Array.from({length:s.length},(_,i)=>s.charCodeAt(i));const U={kind:'undefined'},N={kind:'null'},B=v=>({kind:'boolean',value:v}),O=id=>({kind:'object',id}),S=v=>({kind:'string',units:u(v)}),F=v=>{const b=Buffer.alloc(8);b.writeDoubleBE(v);return{kind:'number',bits:b.toString('hex')}};
const symbol='BTC/\ud800';const rows=[];
function run(method,label,patch={}) {
 const input={method,label,id:'123456789012345678901234567890',readId:S('order/\ud800'),symbol:u(symbol),wall:F(600001),clock:F(8000),version:F(2),fetchVersion:F(2),last:F(6000),promise:U,cooldown:U,cached:U,configured:U,tier:S('premium'),seedCount:0,emptyOldest:false,entries:[[u('order/\ud800'),O('I')]],positions:[O('P0'),O('P1')],throwAt:null,action:null,objects:{A:{},L:{},I:{createdAt:F(1),side:S('buy'),symbol:S(symbol),'Symbol(proof)':O('L')},P0:{symbol:S(symbol),contracts:F(0),leverage:F(3),marginType:F(1)},P1:{symbol:S(symbol),contracts:F(2),leverage:F(7),marginType:F(0)},M:{limits:O('ML')},ML:{leverage:O('LL')},LL:{min:F(1),max:F(20)}}};
 Object.assign(input,patch);if(patch.props){for(const[id,props]of Object.entries(patch.props))Object.assign(input.objects[id],props);delete input.props;}
 const objects={},raw={},ids=new WeakMap(),trace=[],calls=[],token={};let version,clock,last,promise,hints,oldHints,recent={old:true},findCount=0;const sym=Symbol('proof');
 function tag(v){if(v===undefined)return U;if(v===null)return N;if(typeof v==='number')return F(v);if(typeof v==='boolean')return B(v);if(typeof v==='string')return{kind:'string',units:u(v)};if(typeof v==='object'&&ids.has(v))return O(ids.get(v));throw Error('unclassified result');}
 function decode(v){switch(v.kind){case'undefined':return undefined;case'null':return null;case'number':return Buffer.from(v.bits,'hex').readDoubleBE();case'boolean':return v.value;case'string':return String.fromCharCode(...v.units);case'object':return objects[v.id];default:throw Error(v.kind)}}
 function touch(event){trace.push(event);if(input.throwAt===event)throw token;if(input.action==='replaceHints'&&event==='clock:wall')hints=new Map();if(input.action==='changeLast'&&event==='clock:virtual')last=7999;if(input.action==='changeExposed'&&event==='find:2')raw.P1.leverage=11;if(input.action==='changeFinite'&&event==='lookup:NumberFinite')raw.P1.leverage=NaN;}
 for(const id of Object.keys(input.objects)){raw[id]={};objects[id]=new Proxy(raw[id],{get(t,key,r){touch(`property:${id}:${String(key)}`);return Reflect.get(t,key,r)}});ids.set(objects[id],id);}
 for(const[id,props]of Object.entries(input.objects))for(const[key,v]of Object.entries(props))raw[id][key]=decode(v);
 raw.I[sym]=objects.A;
 version=decode(input.version);clock=decode(input.clock);last=decode(input.last);promise=decode(input.promise);
 hints=new Map();if(input.emptyOldest)hints.set('',objects.A);for(let i=0;i<input.seedCount;i++)hints.set(`seed/${i}`,objects.A);for(const[key,v]of input.entries)hints.set(String.fromCharCode(...key),decode(v));oldHints=hints;
 function bound(kind,receiver,fn){touch('lookup:'+kind);return function(...args){assert.equal(this,receiver,kind+' receiver');touch('call:'+kind);if(kind==='HintSet'){ids.set(args[1],'H');raw.H=args[1];objects.H=args[1];}calls.push({kind,args:args.map(tag)});return fn(...args)}}
 function mapView(map,margin=false){const view={};Object.defineProperties(view,{get:{get(){return bound(margin?'MarginGet':'HintGet',view,k=>margin?decode(input.cached):map.get(k))}},set:{get(){return bound('HintSet',view,(k,v)=>map.set(k,v))}},delete:{get(){return bound('HintDelete',view,k=>map.delete(k))}},size:{get(){touch('size:hints');return map.size}},keys:{value(){touch('keys:hints');return map.keys()}}});return view;}
 const env={exports:{},LIGHTER_ORDER_RECONCILIATION_COOLDOWN_MS:3000,Date:{now(){touch('clock:wall');return decode(input.wall)}},virtualClock:{getCurrentTimeMs(){touch('clock:virtual');return clock}}};
 Object.defineProperty(env,'Boolean',{get(){touch('lookup:Boolean');return v=>{touch('call:Boolean');calls.push({kind:'Boolean',args:[tag(v)]});return Boolean(v)}}});
 Object.defineProperty(env,'extractLighterAccountTierFromPayload',{get(){touch('lookup:TierExtract');return v=>{touch('call:TierExtract');calls.push({kind:'TierExtract',args:[tag(v)]});return decode(input.tier)}}});
 env.Number=new Proxy(Number,{get(t,k){if(k==='isFinite'){touch('lookup:NumberFinite');return v=>{touch('call:NumberFinite');calls.push({kind:'NumberFinite',args:[tag(v)]});return Number.isFinite(v)}}return t[k]}});
 env.Math=new Proxy(Math,{get(t,k){const kind=k==='min'?'MathMin':'MathMax';touch('lookup:'+kind);return(...args)=>{touch('call:'+kind);calls.push({kind,args:args.map(tag)});return Math[k](...args)}}});
 const Subject=new Function('env',`with(env){${compiled.outputText};return exports.Subject;}`)(env),api=new Subject();
 Object.defineProperties(api,{placedOrderHints:{get(){touch('owner:hints');return mapView(hints)}},marginModeBySymbol:{get(){touch('owner:margin');return mapView(null,true)}},accountSnapshotCacheVersion:{get(){touch('get:version');return version},set(v){touch('set:version');version=v}},recentAccountSnapshot:{set(v){touch('set:recent');recent=v}},accountLimits:{get(){touch('get:limits');return objects.L}},ordersFetchPromise:{get(){touch('get:promise');return promise}},lastOrdersFetchAt:{get(){touch('get:last');return last}}});
 for(const[k,kind,fn]of [['marginTypeToMarginMode','MarginTypeToMode',v=>v],['getMarketConfiguredMarginMode','MarketMode',()=>decode(input.configured)],['snapToLeverageTier','SnapTier',v=>v]])Object.defineProperty(api,k,{get(){return bound(kind,api,fn)}});
 const positions=input.positions.map(v=>v.kind==='hole'?undefined:decode(v));for(let i=0;i<input.positions.length;i++)if(input.positions[i].kind==='hole')delete positions[i];
 Object.defineProperty(api,'store',{get(){
   touch('owner:store');
   return {
     get marketsBySymbol(){touch('get:markets');return new Proxy({[symbol]:objects.M},{get(t,k){touch('get:market');return t[k]}})},
     get positions(){
       touch('get:positions');
       return new Proxy(positions,{get(t,k,r){
         if(k==='find'){
           touch('lookup:find');
           return function(predicate){assert.equal(this,r,'find receiver');findCount++;touch('find:'+findCount);return Array.prototype.find.call(r,predicate)};
         }
         return Reflect.get(t,k,r);
       }});
     }
   };
 }});
 let result,error=null;try{switch(method){case'rememberPlacedOrderHint':{const id=BigInt(input.id);const old=BigInt.prototype.toString;try{BigInt.prototype.toString=function(){touch('string:orderId');return old.call(this)};result=api[method](id,objects.I);}finally{BigInt.prototype.toString=old;}break;}case'readPlacedOrderHint':result=api[method](decode(input.readId));break;case'rememberRecentAccountSnapshot':result=api[method](objects.A,decode(input.fetchVersion));break;case'shouldSkipOrderReconciliationFetch':result=api[method](decode(input.cooldown));break;case'getCurrentMarginModeForSymbol':result=api[method](symbol);break;case'getCurrentLeverageForSymbol':result=api[method](symbol,objects.M);break;default:result=api[method]();}}catch(e){error=e===token?{kind:'token'}:{kind:'generated',name:e.name,message:u(e.message)}}
 // H is the ordinary spread result. Identify after execution without invoking getters.
 const newKey=input.id;for(const map of [oldHints,hints])if(map.has(newKey)){const v=map.get(newKey);if(v&&!ids.has(v)){ids.set(v,'H');raw.H=v;objects.H=v;}}
 // Calls may tag the newly constructed hint before we can inspect it.
 const mapEntries=m=>Array.from(m,([k,v])=>({key:u(k),value:tag(v)}));
 const snapshot=recent?.old?{kind:'old'}:recent===undefined?{kind:'undefined'}:{kind:'recent',fetchedAt:tag(recent.fetchedAt),cacheVersion:F(recent.cacheVersion),value:tag(recent.value)};
 const copied=raw.H?Object.entries(raw.H).map(([key,value])=>({key,value:tag(value)})):[];const copySymbol=raw.H?tag(raw.H[sym]):U;const copiedSymbolKeys=raw.H?Object.getOwnPropertySymbols(raw.H).map(k=>{assert.equal(k,sym,'actual source symbol key identity');return 'proof'}):[];
 rows.push({input,result:error?null:tag(result),error,trace,calls,version:F(version),snapshot,currentMap:mapEntries(hints),oldMap:mapEntries(oldHints),sameMap:hints===oldHints,copied,copySymbol,copiedSymbolKeys});
}
// Each family is a bounded authored floor, not an exhaustive census.
for(const [label,p]of [['ordinary',{}],['atLimit',{seedCount:1998}],['eviction',{seedCount:2002}],['emptyOldest',{seedCount:2001,emptyOldest:true}],['replacement',{action:'replaceHints'}]])run(names[0],label,p);
for(const event of ['lookup:HintSet','string:orderId','property:I:side','clock:wall','call:HintSet','size:hints','keys:hints','call:HintDelete'])run(names[0],'throw/'+event,{throwAt:event,seedCount:2001});
for(const[label,p]of [['boundary',{}],['expired',{wall:F(600002)}],['negative',{wall:F(-2)}],['nan',{wall:F(NaN)}],['missing',{entries:[]}],['falseHint',{entries:[[u('order/\ud800'),F(0)]]}],['undefinedId',{readId:U}],['emptyId',{readId:S('')}],['replaceOnClock',{wall:F(600002),action:'replaceHints'}]])run(names[1],label,p);
for(const event of ['lookup:HintGet','call:HintGet','clock:wall','property:I:createdAt','call:HintDelete'])run(names[1],'throw/'+event,{throwAt:event,wall:F(600002)});
for(const[label,p]of [['matching',{}],['stale',{fetchVersion:F(1)}],['nan',{version:F(NaN),fetchVersion:F(NaN)}],['signedZero',{version:F(-0),fetchVersion:F(0)}],['clockThrow',{throwAt:'clock:virtual'}],['setterThrow',{throwAt:'set:recent'}]])run(names[2],label,p);
for(const v of [2,-0,2**53,Infinity,NaN])run(names[3],'version/'+String(v),{version:F(v)});
for(const event of ['get:version','set:version','set:recent'])run(names[3],'throw/'+event,{throwAt:event});
for(const[label,p]of [['tier',{}],['undefined',{tier:U}],['null',{tier:N}],['lookupThrow',{throwAt:'lookup:TierExtract'}],['argumentThrow',{throwAt:'get:limits'}],['callThrow',{throwAt:'call:TierExtract'}]])run(names[4],label,p);
for(const[label,p]of [['inside',{}],['boundary',{last:F(5000)}],['outside',{last:F(4999)}],['zeroLast',{last:F(0)}],['negativeLast',{last:F(-1)}],['nanLast',{last:F(NaN)}],['busy',{promise:O('A')}],['nullCooldown',{cooldown:N}],['zeroCooldown',{cooldown:F(0)}],['nanCooldown',{cooldown:F(NaN)}],['changedLast',{action:'changeLast'}],['booleanLookupThrow',{throwAt:'lookup:Boolean'}],['clockThrow',{throwAt:'clock:virtual'}]])run(names[5],label,p);
for(const[label,p]of [['active',{}],['cachedZero',{cached:F(0)}],['cachedOne',{cached:F(1)}],['cachedNull',{cached:N}],['configured',{positions:[],configured:F(1)}],['first',{positions:[O('P0')]}],['default',{positions:[]}],['otherSymbol',{props:{P1:{symbol:S('other')},P0:{symbol:S('other')}}}],['hole',{positions:[{kind:'hole'}]}],['convertThrow',{throwAt:'lookup:MarginTypeToMode'}]])run(names[6],label,p);
for(const[label,p]of [['exposed',{}],['fallback',{positions:[O('P0')]}],['marketMaximum',{positions:[]}],['defaultTen',{positions:[],props:{LL:{max:U}}}],['finiteOnly',{props:{P1:{leverage:F(Infinity)}}}],['clampedHigh',{props:{P1:{leverage:F(30)}}}],['clampedLow',{props:{P1:{leverage:F(-4)}}}],['changeExposed',{action:'changeExposed'}],['changeFinite',{action:'changeFinite'}],['hole',{positions:[{kind:'hole'}]}],['snapThrow',{throwAt:'call:SnapTier'}],['minLookupThrow',{throwAt:'lookup:MathMin'}]])run(names[7],label,p);
assert.equal(rows.length,13+14+6+8+6+13+10+12,'independent authored per-method case floor');
const payload={members,rows};
const contentHash=require('node:crypto').createHash('sha256').update(JSON.stringify(payload)).digest('hex');
process.stdout.write(JSON.stringify({...payload,contentHash}));
