const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH??'typescript');
const sourcePath=path.resolve(__dirname,'../../../packages/safe-cex/src/exchanges/coinbase/coinbase.exchange.ts');
const sourceText=fs.readFileSync(sourcePath,'utf8');
const source=ts.createSourceFile(sourcePath,sourceText,ts.ScriptTarget.Latest,true,ts.ScriptKind.TS);
const exchange=source.statements.find(node=>ts.isClassDeclaration(node)&&node.name?.text==='CoinbaseExchange');
if(!exchange)throw Error('missing CoinbaseExchange declaration');
const defaultMessage=source.statements
 .filter(ts.isVariableStatement)
 .flatMap(node=>node.declarationList.declarations)
 .find(node=>ts.isIdentifier(node.name)&&node.name.text==='COINBASE_PRIVATE_USER_STREAM_DEGRADED_MESSAGE')
 ?.initializer;
if(!defaultMessage||!ts.isStringLiteral(defaultMessage))throw Error('missing literal private stream degraded message');
const authMessage=source.statements
 .filter(ts.isVariableStatement)
 .flatMap(node=>node.declarationList.declarations)
 .find(node=>ts.isIdentifier(node.name)&&node.name.text==='COINBASE_PRIVATE_USER_STREAM_AUTH_DEGRADED_MESSAGE')
 ?.initializer;
if(!authMessage||!ts.isStringLiteral(authMessage))throw Error('missing literal private stream auth degraded message');
function method(name,receiver,now){
 const declaration=exchange.members.find(node=>ts.isPropertyDeclaration(node)&&node.name.getText(source)===name);
 if(!declaration?.initializer)throw Error('missing CoinbaseExchange.'+name);
 const expression=ts.transpileModule(declaration.initializer.getText(source),{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.None}}).outputText
  .replace(/^"use strict";\s*/, '').replace(/;\s*$/, '');
 const factory=vm.runInNewContext('(function(){return ('+expression+');})',{
  Date:{now:()=>now},
  LogSeverity:{Warning:'warning'},
  COINBASE_PRIVATE_USER_STREAM_DEGRADED_MESSAGE:defaultMessage.text,
  COINBASE_PRIVATE_USER_STREAM_AUTH_DEGRADED_MESSAGE:authMessage.text,
 });
 return factory.call(receiver);
}
let now=100;
const checkpoints=[];
const healthFlags=['publicHealthy','privateHealthy','publicInitialized','privateInitialized'];
const healthIdentityCases=healthFlags.map(flag=>({label:'field-identity-supplement',flag,value:Object.fromEntries(healthFlags.map(name=>[name,name===flag]))}));
const state={
 lastPrivateUserStreamMessageAt:0,
 hasWarnedPrivateUserStreamDegraded:false,
 store:{
  socketHealth:{publicHealthy:true,privateHealthy:false,publicInitialized:true,privateInitialized:false,privateDegradedReason:'stale'},
  notifications:[],
  updateSocketHealth(data){this.socketHealth={...this.socketHealth,...data};this.notifications.push(structuredClone(this.socketHealth));},
 },
 effects:[],
 log(message,severity){this.effects.push(['log',message,severity]);},
 emitter:{emit:(_event,...args)=>state.effects.push(['warning',...args])},
};
state.markPrivateUserStreamDegraded=method('markPrivateUserStreamDegraded',state,now);
function received(at){now=at;method('markPrivateUserStreamMessageReceived',state,now)();}
function degraded(at,reason,force){now=at;method('markPrivateUserStreamDegraded',state,now)(reason,force);}
received(100);
checkpoints.push({lastMessageAt:state.lastPrivateUserStreamMessageAt,hasWarned:state.hasWarnedPrivateUserStreamDegraded,socketHealth:structuredClone(state.store.socketHealth)});
degraded(200,'private stream stalled',true);
checkpoints.push({lastMessageAt:state.lastPrivateUserStreamMessageAt,hasWarned:state.hasWarnedPrivateUserStreamDegraded,socketHealth:structuredClone(state.store.socketHealth)});
degraded(300,'private stream still stalled',true);
checkpoints.push({lastMessageAt:state.lastPrivateUserStreamMessageAt,hasWarned:state.hasWarnedPrivateUserStreamDegraded,socketHealth:structuredClone(state.store.socketHealth)});
received(400);
checkpoints.push({lastMessageAt:state.lastPrivateUserStreamMessageAt,hasWarned:state.hasWarnedPrivateUserStreamDegraded,socketHealth:structuredClone(state.store.socketHealth)});
degraded(500,undefined,true);
checkpoints.push({lastMessageAt:state.lastPrivateUserStreamMessageAt,hasWarned:state.hasWarnedPrivateUserStreamDegraded,socketHealth:structuredClone(state.store.socketHealth)});
method('markPrivateUserStreamAuthFailed',state,now)('invalid signature');
checkpoints.push({lastMessageAt:state.lastPrivateUserStreamMessageAt,hasWarned:state.hasWarnedPrivateUserStreamDegraded,socketHealth:structuredClone(state.store.socketHealth)});
method('markPrivateUserStreamAuthFailed',state,now)(undefined);
checkpoints.push({lastMessageAt:state.lastPrivateUserStreamMessageAt,hasWarned:state.hasWarnedPrivateUserStreamDegraded,socketHealth:structuredClone(state.store.socketHealth)});
method('markPrivateUserStreamAuthFailed',state,now)('');
checkpoints.push({lastMessageAt:state.lastPrivateUserStreamMessageAt,hasWarned:state.hasWarnedPrivateUserStreamDegraded,socketHealth:structuredClone(state.store.socketHealth)});
process.stdout.write(JSON.stringify({
 lastMessageAt:state.lastPrivateUserStreamMessageAt,
 hasWarned:state.hasWarnedPrivateUserStreamDegraded,
 socketHealth:state.store.socketHealth,
 notifications:state.store.notifications,
 effects:state.effects,
 checkpoints,
 healthIdentityCases,
}));
