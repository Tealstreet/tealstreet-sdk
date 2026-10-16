'use strict';
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict'),crypto=require('node:crypto');
assert.ok(process.env.SAFE_CEX_TYPESCRIPT_PATH);const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const root=path.resolve(__dirname,'../../..'),dir=path.join(root,'packages/safe-cex/src/exchanges/orderly'),hashes={};
function source(file){const s=fs.readFileSync(path.join(dir,file),'utf8');hashes[file]=crypto.createHash('sha256').update(s).digest('hex');return ts.createSourceFile(file,s,ts.ScriptTarget.Latest,true);}
const asts={public:source('orderly.ws-public.ts'),private:source('orderly.ws-private.ts')};
const typeAst=source('orderly.types.ts');const urlDecl=typeAst.statements.filter(ts.isVariableStatement).flatMap(s=>[...s.declarationList.declarations]).find(d=>d.name.getText(typeAst)==='BASE_WS_URL');assert.ok(urlDecl);const BASE_WS_URL=vm.runInNewContext('('+urlDecl.initializer.getText(typeAst)+')');
function body(side,name,bindings,receiver){const ast=asts[side];const cls=ast.statements.find(s=>ts.isClassDeclaration(s)&&s.name.text===(side==='public'?'OrderlyPublicWebsocket':'OrderlyPrivateWebSocket'));assert.ok(cls);const member=cls.members.filter(m=>m.name?.getText(ast)===name);assert.equal(member.length,1);assert.ok(ts.isArrowFunction(member[0].initializer));const result=ts.transpileModule('function make(){return ('+member[0].initializer.getText(ast)+')}',{compilerOptions:{target:ts.ScriptTarget.ES2022},reportDiagnostics:true});assert.equal((result.diagnostics||[]).filter(d=>d.category===ts.DiagnosticCategory.Error).length,0);return vm.runInNewContext(result.outputText+';make',bindings).call(receiver);}
function capture(v){if(v===undefined)return {undefined:true};if(typeof v==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(v);return {number:Number.isNaN(v)?'7ff8000000000000':b.toString('hex')};}if(Array.isArray(v))return v.map(capture);if(v&&typeof v==='object')return Object.fromEntries(Object.entries(v).map(([k,x])=>[k,capture(x)]));return v;}
const {defaults,cases}=require('./ws-controls-fixtures.cjs'),rows={};
for(const [family,configs]of Object.entries(cases)){const [side,name]=family.split('.');rows[family]=[];for(const raw of configs){const cfg=structuredClone({...defaults,...raw}),events=[],original=new Error('controlled failure');let sendCount=0,encoded,parsed=cfg.parsed;
 const effect=(kind,details={})=>{events.push({kind,...capture(details)});if(cfg.fail===kind)throw original;};
 const options={testnet:cfg.testnet,applicationId:cfg.applicationId,key:cfg.key,secret:cfg.secret};if(cfg.applicationIdMissing)delete options.applicationId;
 const parent={options,fetchAndSetPrivateSnapshot(){effect('snapshot',{receiver:this===parent});}};
 const receiver={parent,isDisposed:cfg.disposed||false,topics:cfg.topics,pingAt:'old',ws:'old',messageHandlers:{}};
 const socket={addEventListener(event,callback){effect('listener:'+event,{socket:this===socket,identity:callback===receiver[event==='open'?'onOpen':event==='message'?'onMessage':'onClose']});}};
 function send(value){effect('send:'+ ++sendCount,{log:side==='private',receiver:this===receiver,encoded:value===encoded,value});if(cfg.mutateDisposed)receiver.isDisposed=true;}
 receiver.createWebsocket=function(url){effect('create',{receiver:this===receiver,url});return socket;};receiver.send=send;receiver.sendWithLog=send;
 receiver.onClose=()=>{};receiver.onMessage=()=>{};receiver.emitWsReconnected=function(){effect('reconnected',{receiver:this===receiver});};receiver.trackLastReceived=function(){effect('received',{receiver:this===receiver});};
 let appended=false;
 const callback=id=>function(json){effect('callback:'+id,{receiver:this===undefined,identity:json===parsed,json});if(cfg.append&&!appended){appended=true;receiver.messageHandlers.tickers.push(callback('appended'));}if(cfg.mutateHandlers)receiver.messageHandlers.tickersSuffix=[callback('new')];};
 for(const [topic,ids]of cfg.handlers)receiver.messageHandlers[topic]=ids.map(callback);
 const bindings={BASE_WS_URL,ENABLE_RECONNECTING_WEBSOCKET:!!cfg.reconnect,flatten:require(path.join(root,'node_modules/lodash/flatten')),JSON:{stringify(value){encoded=value;effect('encode',{value,keys:Object.keys(value),paramsKeys:value.params?Object.keys(value.params):[]});return value;}},performance:{now(){effect('clock');return cfg.clock??19.25;}},virtualClock:{getCurrentTimeMs(){effect('virtual');return 1723638896789.5;}},orderlySign(message,secret){effect('sign',{message,identity:secret===options.secret});return 'signed';},jsonParse(data){effect('parse',{data});return parsed;}};
 for(const m of ['connectAndSubscribe','onOpen','subscribe','ping',...(side==='public'?['handlePingEvent','onMessage']:[])])receiver[m]=body(side,m,bindings,receiver);
 let error;try{receiver[name]({data:cfg.data});}catch(e){error={identity:e===original,message:e.message};}
 rows[family].push({cfg:capture(cfg),events,error,ws:receiver.ws===socket,pingAt:capture(receiver.pingAt),disposed:receiver.isDisposed});
}}
process.stdout.write(JSON.stringify({hashes,rows}));
