import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {fileURLToPath} from 'node:url';
const require=createRequire(import.meta.url);
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../../..');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||path.join(root,'node_modules/typescript'));
const file=path.join(root,'packages/safe-cex/src/exchanges/bitget/bitget.ws-private.ts');
const ast=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
const owner=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text==='BitgetPrivateWebsocket');
const fields=['recoverMissingPrivateStream','onOpen'].map(name=>{
 const f=owner.members.find(n=>ts.isPropertyDeclaration(n)&&n.name.getText(ast)===name);
 assert(f?.initializer,'RECOVERY actual source '+name);return name+'='+f.initializer.getText(ast)+';';
}).join('\n');
const logging=ts.transpileModule(fs.readFileSync(path.join(root,'packages/logging/src/LogSeverity.ts'),'utf8').replace(/export\s+/g,''),{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText;
const severity=new Function(logging+';return LogSeverity;')();
const code=ts.transpileModule('class Source {'+fields+'}',{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText;
const cases=JSON.parse(fs.readFileSync(0,'utf8'));
const decode=x=>x==='nan'?NaN:x==='inf'?Infinity:x==='-inf'?-Infinity:x;
const bits=x=>{const b=Buffer.alloc(8);b.writeDoubleLE(x);return [...b];};
const outputs=cases.map(c=>{
 const trace=[],logs=[],error={units:[65,0xd800,66,0xdc00]};let last=decode(c.last),fallback=c.using,reconnects=0,launches=0;
 const record=n=>{trace.push(n);if(trace.length-1===c.fail)throw error;};
 const routing=()=>{record('routing');return {wsPrivate:{fallback:c.hasFallback?'wss://fallback':undefined}};};
 const Source=new Function('LogSeverity','ENABLE_RECONNECTING_WEBSOCKET','resolveTransportRouting',code+';return Source;')(severity,c.enabled,routing);
 const target=new Source();
 Object.defineProperties(target,{
  isDisposed:{get(){record('disposed');return c.disposed;}},
  lastPrivateStreamRecoveryAt:{get(){record('last');return last;},set(v){record('set-last');last=v;}},
  privateStreamRecoveryCooldownMs:{get(){record('cooldown');return decode(c.cooldown);}},
  useFallbackUrl:{get(){record('using');return fallback;},set(v){assert.equal(v,true);record('set-using');fallback=v;}}
 });
 target.parent={name:'bitget',options:{},get usesUtaRouting(){record('uta');return c.uta;},log(message,s){assert.equal(s,severity.Warning);logs.push({units:Array.from({length:message.length},(_,i)=>message.charCodeAt(i)),severity:s});record('log');}};
 target.reconnect=()=>{record('reconnect');reconnects++;};
 target.auth=()=>record('auth');target.ping=()=>record('ping');
 target.executeOrdersFetch=()=>{record('launch');launches++;return {then(){throw Error('RECOVERY orders must not await');}};};
 const oldNow=Date.now;Date.now=()=>{record('now');return decode(c.now);};
 let thrown=false;
 try {if(c.op==='open')target.onOpen();else target.recoverMissingPrivateStream(String.fromCharCode(...c.reason));}
 catch(e){assert.equal(e,error);thrown=true;}
 finally{Date.now=oldNow;}
 return {trace,logs,lastBits:bits(last),using:fallback,reconnects,launches,thrown};
});
assert.equal(outputs.length,403,'RECOVERY independent input floor');
console.log(JSON.stringify(outputs));
