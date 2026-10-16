const families=[['FuturesPublic','kraken.ws-public.ts','KrakenPublicWebsocket'],['FuturesPrivate','kraken.ws-private.ts','KrakenPrivateWebsocket'],['SpotPublic','krakenspot.ws-public.ts','KrakenSpotPublicWebsocket'],['SpotPrivate','krakenspot.ws-private.ts','KrakenSpotPrivateWebsocket']];
function connectCase(family,file,className,config){
 const trace=[],listeners=[];const original=new Error('connection failure');original.name='ConnectionError';
 const input={marker:17.25};let receiver;
 const fail=point=>{if(config.fail===point)throw original};
 const socket=id=>({id,get readyState(){trace.push({kind:'ready',id});fail('ready');if(config.replaceReady)receiver.ws=socket(41);if(config.clearReady)receiver.ws=undefined;return config.closed?3:config.nan?NaN:1},close(){trace.push({kind:'socket-close',id});fail('socket-close')},addEventListener(event,callback){trace.push({kind:'add',id,event,identity:event==='open'?callback===onOpen:event==='message'?callback===onMessage:true});listeners.push({event,callback});fail('add-'+event);if(config.replaceAdd===event)receiver.ws=socket(37);if(config.clearAdd===event)receiver.ws=undefined;}});
 const onOpen=value=>{trace.push({kind:'callback',event:'open',identity:value===input});fail('open')};
 const onMessage=value=>{trace.push({kind:'callback',event:'message',identity:value===input});fail('message')};
 receiver={isDisposed:!!config.disposed,parent:{options:{testnet:!!config.testnet}},ws:config.old?socket(7):undefined,onClose(){trace.push({kind:'host-close'});fail('host-close')},createWebsocket(url){trace.push({kind:'create',url});fail('create');if(config.disposeCreate)receiver.isDisposed=true;return socket(31)}};
 Object.defineProperty(receiver,'onOpen',{get(){if(config.replaceOpen)receiver.ws=socket(43);return onOpen}});
 receiver.onMessage=onMessage;
 const sourceFile=path.join(src,'exchanges/kraken',file),bindings={BASE_WS_URL:family.startsWith('Spot')?spotTypes.BASE_WS_URL:futuresTypes.BASE_WS_URL,WebSocket:{CLOSED:3}};
 const fn=method(sourceFile,className,'connectAndSubscribe',bindings,receiver);
 let failure=null;try{fn();if(config.clearFire)receiver.ws=undefined;if(config.replaceFire)receiver.ws=socket(39);for(const event of config.fire||['open','message','close','error']){const listener=listeners.find(l=>l.event===event);if(listener)listener.callback(input)}}catch(error){failure={name:error.name,message:error.message,identity:error===original}};
 return {config,trace,failure,slot:receiver.ws?.id??null,disposed:receiver.isDisposed};
}
function disposeCase(config){
 const trace=[];const original=new Error('disposal failure');original.name='DisposalError';
 const fail=point=>{if(config.fail===point)throw original};
 const timeouts=new Map(config.empty?[]:[['a',{id:17}],['b',{id:23}]]),active=new Map(config.empty?[]:[['雪',true],['\ud800',true]]),candles=new Map(config.empty?[]:[['c',{marker:29.125}]]);
 const held=[...timeouts.values()];let didTimeout=false,didActive=false;
 const nativeSet=active.set.bind(active);active.set=(key,value)=>{trace.push({kind:'inactive',key:Array.from({length:key.length},(_,i)=>key.charCodeAt(i)),value});fail('inactive');nativeSet(key,value);if(config.mutateActive&&!didActive){didActive=true;active.delete('\ud800');nativeSet('added',true)}return active};
 for(const[name,map]of[['timeouts',timeouts],['active',active],['candles',candles]]){const clear=map.clear.bind(map);map.clear=()=>{trace.push({kind:'clear',map:name});fail('clear-'+name);clear()}};
 const text=fs.readFileSync(futuresFile,'utf8');hashes[path.basename(futuresFile)]=crypto.createHash('sha256').update(text).digest('hex');
 const ast=ts.createSourceFile(futuresFile,text,ts.ScriptTarget.Latest,true),cls=ast.statements.find(s=>ts.isClassDeclaration(s)&&s.name.text==='KrakenExchange'),member=cls.members.filter(m=>m.name?.getText(ast)==='dispose');assert.equal(member.length,1);
 const body=ts.transpileModule('class Fixture extends Base { '+member[0].getText(ast)+' };new Fixture()', {compilerOptions:{target:ts.ScriptTarget.ES2022},reportDiagnostics:true});assert.equal(body.diagnostics.filter(d=>d.category===ts.DiagnosticCategory.Error).length,0);
 class Base{dispose(){trace.push({kind:'base',timeouts:timeouts.size,active:active.size,candles:candles.size});fail('base')}};
 const receiver=vm.runInNewContext(body.outputText,{Base,clearTimeout(value){trace.push({kind:'timeout',id:value.id,identity:held.includes(value)||value.id===31});fail('timeout');if(config.mutateTimeout&&!didTimeout){didTimeout=true;timeouts.delete('b');timeouts.set('new',{id:31})}}});
 Object.assign(receiver,{ohlcvPollingTimeouts:timeouts,ohlcvPollingActive:active,lastCandleCache:candles});
 let failure=null;try{receiver.dispose()}catch(error){failure={name:error.name,message:error.message,identity:error===original}};
 return {config,trace,failure,timeouts:[...timeouts.values()].map(t=>t.id),active:[...active].map(([key,value])=>({key:Array.from({length:key.length},(_,i)=>key.charCodeAt(i)),value})),candles:candles.size};
}
function lifecycleCorpus(){
 const connectionConfigs=[{},{testnet:true},{disposed:true},{disposed:true,old:true},{old:true},{closed:true},{nan:true},{clearFire:true},{replaceFire:true},{replaceReady:true},{clearReady:true},{disposeCreate:true},{replaceAdd:'open'},{clearAdd:'open'},{replaceOpen:true},...['create','add-open','add-message','add-close','add-error','ready','socket-close','host-close','open','message'].map(fail=>({fail}))];
 const disposalConfigs=[{},{empty:true},{mutateTimeout:true},{mutateActive:true},{mutateTimeout:true,mutateActive:true},...['timeout','inactive','clear-timeouts','clear-active','clear-candles','base'].map(fail=>({fail}))];
 return {connections:Object.fromEntries(families.map(([family,file,cls])=>[family,connectionConfigs.map(config=>connectCase(family,file,cls,config))])),disposal:disposalConfigs.map(disposeCase)};
}
