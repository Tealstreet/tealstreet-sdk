const fs=require('fs'),path=require('path'),vm=require('vm'),assert=require('assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const root=path.resolve(__dirname,'../../..'),file='packages/safe-cex/src/exchanges/hyperliquid/hyperliquid.exchange.ts';
const text=fs.readFileSync(path.join(root,file),'utf8'),ast=ts.createSourceFile(file,text,ts.ScriptTarget.Latest,true);
const cls=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text==='HyperliquidExchange');
const members=cls.members.filter(n=>['start','tick'].includes(n.name?.getText(ast)));
assert.equal(members.length,2,'independent source lifecycle member floor');
const severityFile='packages/logging/src/LogSeverity.ts';const ctx={exports:{}};
vm.runInNewContext(ts.transpileModule(fs.readFileSync(path.join(root,severityFile),'utf8'),{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,ctx);
let active;class Clock extends Date {static now(){return 200000}}
const realm={Date:Clock,LogSeverity:ctx.exports.LogSeverity,loop:(cb,interval)=>{active.record('loop',interval);active.loop=cb}};
vm.createContext(realm);vm.runInContext(ts.transpileModule('class Lifecycle {'+members.map(n=>n.getText(ast)).join('\n')+'};globalThis.Lifecycle=Lifecycle;',{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,realm);
function tag(v){if(v===undefined)return ['u'];if(v===null)return ['null'];if(typeof v==='boolean')return ['bool',v];if(typeof v==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(v);return ['number',b.toString('hex')]}if(typeof v==='string')return ['string',Array.from({length:v.length},(_,i)=>v.charCodeAt(i))];if(Array.isArray(v))return ['array',Array.from(v,tag)];return ['object',Object.keys(v).map(k=>[k,tag(v[k])])];}
const starts=['private','external','public','public-external','no-key','resolve-success','resolve-disposed','mode-disposed','markets-disposed','init-error','markets-error','reject-all','core-null','no-track','truthy-public-option','log-error','drop-observer'];
const ticks=['both','neither','equal-order','equal-position','no-dex','not-needed','disposed','public','reject-all','sync-orders-error','dispose-on-orders','fire-replacement'];
async function run(family,scene){const effects=[],nativeEffects=[],record=(name,...args)=>{nativeEffects.push([name,...args]);effects.push(tag([name,...args]));};active={record};
 const e=new realm.Lifecycle(),markets=[{symbol:'BTC/USDC'}],tickers=[{id:'BTC'}],positions=[{symbol:'native-position'}],core={marker:'core-state'},marker='failure\ud800',store={markets,tickers,loaded:{markets:false,custom:true},update(v){record('update',v);Object.assign(this,v)}};
 Object.assign(e,{options:{key:scene==='no-key'?'':'key',publicDataOnly:scene.startsWith('public')?true:scene==='truthy-public-option'?'yes':false},hasPrivateSessionIdentity:!['no-key','resolve-success','resolve-disposed'].includes(scene),isPublicDataOnly:scene.startsWith('public'),isDisposed:scene==='disposed',store,
 hip3DexNames:scene==='no-dex'?[]:['dex'],_hip3OrdersNeedRefresh:!['not-needed','neither'].includes(scene),_lastOrderRefreshTime:scene==='equal-order'?190000:scene==='neither'?199999:0,_lastPositionRefreshTime:scene==='equal-position'?80000:scene==='neither'?199999:0,HIP3_ORDER_REFRESH_INTERVAL_MS:10000,HIP3_POSITION_REFRESH_INTERVAL_MS:120000});
 e.initExternalPublicData=()=>{record('init');return scene==='init-error'?Promise.reject(marker):Promise.resolve(scene.includes('external'))};
 e.resolveApplicationIdFromKey=()=>{record('resolveApplicationIdFromKey');if(scene==='resolve-disposed')e.isDisposed=true;if(scene==='resolve-success')e.hasPrivateSessionIdentity=true;return Promise.resolve()};
 e.warnOnMismatchedApiWallet=(...a)=>record('warnOnMismatchedApiWallet',...a);
 let release;for(const name of ['resolveUserAbstractionMode','fetchCorePerpsState','fetchBuilderFeeApproval','refreshOrders','fetchAllHip3Positions'])e[name]=()=>{record(name);if(scene==='sync-orders-error'&&name==='refreshOrders')throw marker;if(scene==='dispose-on-orders'&&name==='refreshOrders')e.isDisposed=true;if(scene==='mode-disposed'&&name==='resolveUserAbstractionMode')e.isDisposed=true;if(scene==='drop-observer'&&name==='resolveUserAbstractionMode')return new Promise(r=>release=r);if(scene==='reject-all'||scene===`reject-${name}`)return Promise.reject(marker);return Promise.resolve(name==='fetchCorePerpsState'?(scene==='core-null'?null:core):undefined)};
 e.fetchMarketsAndTickers=()=>{record('markets');if(scene==='markets-disposed')e.isDisposed=true;return scene==='markets-error'?Promise.reject(marker):Promise.resolve([markets,tickers])};
 e.publicWebsocket={connectAndSubscribe(){record('public-connect')}};
 e.privateWebsocket={connectAndSubscribe(){record('private-connect')},applyCoreClearinghouseState(v,flag){assert.equal(v,core,'source core state original handle');record('applyCoreClearinghouseState',v,flag)}};
 e.buildMarketsLowerCaseMap=v=>{assert.equal(v,markets,'source market map original handle');record('buildMarketsLowerCaseMap')};
 e.generateFakePositionsFromMarkets=v=>{assert.equal(v,markets,'source fake positions original markets');record('fake');return positions};
 e.shouldTrackSpotHoldings=()=>{record('shouldTrackSpotHoldings');return scene!=='no-track'};
 e.requestUnifiedSpotCollateralRefresh=(...a)=>record('requestUnifiedSpotCollateralRefresh',...a);
 e.log=(...a)=>{record('log',...a);if(scene==='log-error')throw marker};
 e.getTickInterval=()=>{record('interval');return 2000};
 const actualTick=e.tick;if(family==='start')e.tick=()=>{record('tick');return Promise.resolve()};
 let rejected=false,error,result;const op=family==='start'?e.start():actualTick.call(e);
 if(scene==='drop-observer'){for(let i=0;i<8;i++)await Promise.resolve();assert(release,'source dropped observer reaches owned mode await');release();}
 try{result=await op}catch(err){rejected=true;error=err;assert.equal(error,marker,'source original rejection value')}
 for(let i=0;i<8;i++)await Promise.resolve();
 if(scene==='fire-replacement'){const promise=Promise.resolve('replacement');e.tick=function(){record('loop-fired',this===e);return promise};assert.equal(active.loop.call({}),promise,'source timer original result identity');}
 if(family==='tick') {
 const order=!['neither','equal-order','no-dex','not-needed','disposed','public'].includes(scene),position=!['neither','equal-position','no-dex','disposed','public','sync-orders-error'].includes(scene);
 assert.equal(nativeEffects.filter(v=>v[0]==='refreshOrders').length,Number(order),'source tick strict order guard '+scene);
 assert.equal(nativeEffects.filter(v=>v[0]==='fetchAllHip3Positions').length,Number(position),'source tick strict position guard '+scene);
 assert.equal(nativeEffects.filter(v=>v[0]==='loop').length,Number(!['disposed','public','sync-orders-error','dispose-on-orders'].includes(scene)),'source tick final disposal guard '+scene);
 } else if(!['resolve-disposed','mode-disposed','init-error','markets-error','markets-disposed','log-error'].includes(scene)) {
 assert.equal(store.positions,positions,'source startup positions original alias');
 for(const key of ['orders','balance','positions'])assert.equal(store.loaded[key],scene!=='no-key','source startup private loaded '+key+'/'+scene);
 const updated=nativeEffects.findIndex(v=>v[0]==='update'),connected=nativeEffects.findIndex(v=>v[0]==='private-connect');
 if(connected>=0)assert(updated<connected,'source startup store before private connection');
 }
 return {family,scene,effects:['array',effects],rejected,error:tag(error),result:tag(result),state:tag({order:e._lastOrderRefreshTime,position:e._lastPositionRefreshTime,needed:e._hip3OrdersNeedRefresh,disposed:e.isDisposed,loaded:store.loaded}),alias:store.positions===positions};
}
(async()=>{const cases=[];let completed=0,last=-1;for(const family of ['start','tick'])for(const scene of family==='start'?starts:ticks){cases.push(await run(family,scene));last=completed++;}assert.equal(completed,29,'independent completed lifecycle floor');assert.equal(last,28,'independent final lifecycle iteration');process.stdout.write(JSON.stringify({enums:tag(ctx.exports),cases,completed,last}));})().catch(e=>{process.stderr.write(String(e.stack||e));process.exitCode=1});
