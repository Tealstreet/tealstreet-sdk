const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path');
const {root,load,ts,dependencies}=require('../../safe-cex-types/scripts/accounts-source.cjs');
const file=path.join(root,'packages/safe-cex/src/exchanges/blofin/blofin.ws-public.ts');
const ast=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
const klass=ast.statements.find(s=>ts.isClassDeclaration(s)&&s.name?.text==='BlofinPublicWebsocket');
const variables=ast.statements.filter(ts.isVariableStatement).flatMap(s=>s.declarationList.declarations);
const evaluate=(node,context={})=>new Function(...Object.keys(context),ts.transpileModule('return '+node.getText(ast),{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText)(...Object.values(context));
const maximum=evaluate(variables.find(d=>d.name.text==='BLOFIN_MAX_SUBSCRIBE_FRAME_BYTES').initializer);
const build=evaluate(variables.find(d=>d.name.text==='buildBlofinSubscribeFrames').initializer,{BLOFIN_MAX_SUBSCRIBE_FRAME_BYTES:maximum});
const staticValues=Object.fromEntries(klass.members.filter(m=>ts.isPropertyDeclaration(m)&&m.modifiers?.some(mod=>mod.kind===ts.SyntaxKind.StaticKeyword)).map(m=>[m.name.text,evaluate(m.initializer)]));
const {BASE_WS_URL}=load(path.join(root,'packages/safe-cex/src/exchanges/blofin/blofin.types.ts'));
const methodNames=['connectAndSubscribe','onOpen','onMarketsChanged','ping','subscribe','sendSubscriptionTopics','armConnectionFailureNotice','markConnectionEstablished'];
const bindArrow=(node,context,host)=>new Function(...Object.keys(context),ts.transpileModule('return function(){return '+node.getText(ast)+'}',{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText)(...Object.values(context)).call(host);
const topic=(channel,instId)=>({channel,instId});
const basic=[topic('tickers','BTC-USDT'),topic('funding-rate','ETH-USDT')];
const size=t=>new TextEncoder().encode(JSON.stringify({op:'subscribe',args:t})).byteLength;
const frameCases=[
 {topics:[]},{topics:[basic[0]]},{topics:basic},{topics:[topic('candle1m','é🚀中'),topic('books','quote"\\\n\u0000')]},
 {topics:basic,max:size(basic)},{topics:basic,max:size(basic)-1},{topics:[basic[0]],max:size([basic[0]])-1},
 {topics:[],max:0},{topics:[basic[0]],max:0},{topics:[basic[0]],max:-1},{topics:basic,max:'nan'},{topics:basic,max:'infinity'},
 {topics:basic,max:size(basic)-0.5},{topics:[basic[0],topic('tickers','x'.repeat(4100))]},
 {topics:Array.from({length:200},(_,i)=>topic('tickers','MKT-'+i))},
 {topics:[topic('tickers','é🚀中')],max:size([topic('tickers','é🚀中')])-1},
 {topics:[{instId:'BTC-USDT',channel:'tickers'}]},
 {topics:[{channel:'tickers',instId:'BTC-USDT',extra:{z:1,a:[null,true,'é']},priority:1.0}]},
 {topics:[{'10':'ten',channel:'tickers','2':'two',instId:'BTC-USDT'}]},
];
const cases=[];const add=(group,actions,initial={})=>cases.push({group,actions,initial});
for(const initial of [{},{testnet:true},{disposed:true},{fail:'create'},{fail:'listener:message'},{topics:[['custom',[topic('books','KEEP')]]]}, {marketsAfterFirst:['CHANGED']}])add('connect',[['connect']],initial);
for(const initial of [{},{disposed:true},{reconnecting:false},{fail:'send'},{fail:'off'},{disposeOnSend:true}])add('open',[['open']],{socket:true,topics:[['tickers',basic]],...initial});
for(const initial of [{},{disposed:true},{socket:false},{fail:'send'}])add('add',[['add',['BTC-USDT','BTC-USDT','é🚀']]],{socket:true,topics:[['tickers',[basic[0]]]],...initial});
add('add',[['add',[]]],{socket:true});
for(const initial of [{},{disposed:true},{fail:'send'},{now:'negative-zero'}])add('ping',[['ping']],initial);
add('subscribe',[['subscribe']],{topics:[['z',[basic[0]]],['10',[topic('books','TEN')]],['2',[topic('books','TWO')]],['a',[basic[1]]]]});
add('subscribe',[['subscribe']]);add('subscribe',[['subscribe']],{disposed:true,topics:[['x',basic]]});
for(const topics of [[],basic,Array.from({length:200},(_,i)=>topic('tickers','MKT-'+i)),[basic[0],topic('tickers','x'.repeat(4100))]])add('send',[['send',topics]]);
add('send',[['send',Array.from({length:200},(_,i)=>topic('tickers','MKT-'+i))]],{fail:'send:2'});
for(const [actions,initial] of [
 [[['arm'],['arm'],['fire',1]],{}],[[['arm']],{disposed:true}],[[['arm']],{notified:true}],[[['arm']],{timeout:5}],
 [[['arm'],['disposed',true],['fire',1]],{}],[[['arm'],['notified',true],['fire',1]],{}],
 [[['arm'],['fire',1]],{fail:'log'}],[[['arm'],['fire',1]],{fail:'warning'}],[[['arm']],{fail:'timer'}],
 [[['arm'],['arm'],['fire',0],['fire',1]],{firstTimer:0}],
 [[['arm'],['mark'],['arm'],['fire',2]],{}],
])add('arm',actions,initial);
for(const initial of [{},{timeout:5,notified:true},{timeout:0,notified:true},{timeout:5,fail:'clear'},{disposed:true,notified:true}])add('mark',[['mark']],initial);
assert.equal(frameCases.length,19,'independent frame-case floor');assert.equal(cases.length,46,'independent host-case floor');
function num(v){const b=Buffer.alloc(8);b.writeDoubleBE(v);return b.toString('hex');}
function run(input){
 const {initial:i,actions}=input,trace=[],sent=[],timers=new Map();let next=i.firstTimer??1,disposed=!!i.disposed,socket=!!i.socket,marketReads=0,sendCount=0;
 const state={topics:Object.fromEntries(structuredClone(i.topics??[])),pingAt:0,connectionFailureNoticeTimeout:i.timeout,connectionFailureNotified:!!i.notified,
 get isDisposed(){return disposed},get ws(){return socket?{addEventListener(event){call('listener:'+event);trace.push(['listener',event]);}}:undefined},set ws(v){socket=!!v},
 store:{get markets(){const ids=marketReads++&&i.marketsAfterFirst?i.marketsAfterFirst:['BTC-USDT','ETH-USDT'];trace.push(['markets',marketReads]);return ids.map(id=>({id}));}},
 parent:{options:{testnet:!!i.testnet},emitter:{off(){call('off');trace.push(['off'])},on(){call('on');trace.push(['on'])},emit(event,message,opts){call('warning');trace.push(['warning',message,opts.forceToast])}},log(message,severity){call('log');assert.equal(severity,'warning');trace.push(['log',message,severity])}},
 createWebsocket(url){call('create');trace.push(['create',url]);return{addEventListener(event){call('listener:'+event);trace.push(['listener',event]);}}},
 send(frame){sendCount++;call('send');call('send:'+sendCount);sent.push(frame==='ping'?{ping:true}:JSON.parse(frame));trace.push(['send',frame]);if(i.disposeOnSend)disposed=true;},
 emitWsReconnected(){call('reconnected');trace.push(['reconnected']);},};
 const context={BASE_WS_URL,ENABLE_RECONNECTING_WEBSOCKET:i.reconnecting!==false,flatten:dependencies('lodash/flatten'),buildBlofinSubscribeFrames:build,performance:{now(){trace.push(['now']);return i.now==='negative-zero'?-0:i.now??123.5}},BlofinPublicWebsocket:staticValues,LogSeverity:{Warning:'warning'},
 setTimeout(callback,delay){call('timer');const id=next++;timers.set(id,callback);trace.push(['timer',id,delay]);return id;},clearTimeout(id){call('clear');timers.delete(id);trace.push(['clear',id]);}};
 for(const name of methodNames){const member=klass.members.find(m=>m.name?.text===name);assert(member&&ts.isArrowFunction(member.initializer),'maintained arrow body '+name);state[name]=bindArrow(member.initializer,context,state);}
 function call(label){if(i.fail===label)throw Error(label+' failed');}
 const snapshots=[],results=[];
 for(const [action,arg] of actions){try{if(action==='connect')state.connectAndSubscribe();else if(action==='open')state.onOpen();else if(action==='add')state.onMarketsChanged({added:arg.map(id=>({id}))});else if(action==='send')state.sendSubscriptionTopics(arg);else if(action==='fire'){const callback=timers.get(arg);timers.delete(arg);callback?.();}else if(action==='disposed')disposed=arg;else if(action==='notified')state.connectionFailureNotified=arg;else state[{arm:'armConnectionFailureNotice',mark:'markConnectionEstablished'}[action]??action]();results.push(null);}catch(e){results.push(e.message);}
 snapshots.push({topics:Object.entries(state.topics),pingBits:num(state.pingAt),timeout:state.connectionFailureNoticeTimeout??null,notified:state.connectionFailureNotified,socket,disposed});}
 return{...input,trace,sent,snapshots,results};
}
const output=cases.map(run);
assert.equal(output.find(c=>c.group==='connect'&&!Object.keys(c.initial).length).snapshots[0].topics[0][1][0].instId,'BTC-USDT','source connect native topic ID');
assert.equal(output.find(c=>c.group==='add'&&!c.initial.fail&&c.initial.socket&&!c.initial.disposed&&c.actions[0][1].length).snapshots[0].topics[0][1].length,4,'source does not deduplicate additions');
assert.equal(output.find(c=>c.group==='send'&&c.actions[0][1].length===2&&c.actions[0][1][1].instId.length===4100).sent.length,0,'source builds all frames before sending');
const aliasHost={topics:{tickers:[topic('tickers','ORIGINAL')]},ws:{},isDisposed:false,sendSubscriptionTopics(){}};
const addBody=bindArrow(klass.members.find(m=>m.name?.text==='onMarketsChanged').initializer,{},aliasHost);const old=aliasHost.topics.tickers;addBody({added:[{id:'NEW'}]});old[0].instId='MUTATED';assert.equal(aliasHost.topics.tickers[0].instId,'MUTATED','source concat retains topic object aliases');
const frames=frameCases.map(input=>{let result;try{result={ok:build(input.topics,input.max==='nan'?NaN:input.max==='infinity'?Infinity:input.max)}}catch(e){result={error:e.message}}return{input,result}});
assert.equal(frames[4].result.ok.length,1,'exact byte limit fits');assert.equal(frames[5].result.ok.length,2,'one byte below splits');assert.equal(frames[6].result.error,'BloFin subscription topic exceeds the maximum WebSocket frame size','single oversized throws');assert.equal(frames[14].result.ok.length,2,'200 topics: independent literal-length packing yields two frames (101/99 topics)');assert.deepEqual(frames[14].result.ok.map(f=>JSON.parse(f).args.length),[101,99],'independent topic partition');
process.stdout.write(JSON.stringify({maximum,frames,cases:output,alias:{sourceFirst:'MUTATED',nativeExpected:'ORIGINAL'},stoppedAfter:{frames:19,hostCases:46,alias:1}}));
