const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const root=path.resolve(__dirname,'../../../packages/safe-cex/src');
function ast(file){return ts.createSourceFile(file,fs.readFileSync(path.join(root,file),'utf8'),ts.ScriptTarget.Latest,true)}
function members(tree,names){const cls=tree.statements.find(n=>ts.isClassDeclaration(n)&&n.members.some(m=>m.name?.getText(tree)===names[0]));return names.map(name=>{const m=cls.members.find(n=>n.name?.getText(tree)===name);assert.ok(m,name);return m.getText(tree)}).join('\n')}
const base=ast('exchanges/base.ws.ts'),venue=ast('exchanges/woo/woo.ws-private.ts');
const classifier=base.statements.find(n=>ts.isFunctionDeclaration(n)&&n.name?.text==='isHeartbeatFrame');assert.ok(classifier);
function execute(source,context){context.exports={};vm.runInNewContext(ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,context);return context.result}
const body=classifier.getText(base)+'\nclass Witness {\n'+members(venue,['ping'])+'\n'+members(base,['send','sendWithLog','emitWsDiagnosticSend'])+'\n}\n';
const cases=[];
for(const disposed of [false,true])for(const ready of [null,0,1,2,3])for(const fail of [false,true])for(const diagnostics of [false,true]){
 const input={disposed,ready,fail,diagnostics};let diagnosticChecks=0;
 const result=execute(body+`
 const socket=new Witness(),sent=[],logs=[];let reconnects=0;
 socket.parent={isDisposed:input.disposed,log:(message,severity)=>logs.push({message,severity})};
 socket.isDisposed=input.disposed;socket.pingAt=7;socket.reconnect=()=>reconnects++;
 socket.shouldDropWsDiagnostic=()=>{throw Error('heartbeat reached diagnostic drop logic')};
 if(input.ready!==null)socket.ws={readyState:input.ready,send:message=>{sent.push(message);if(input.fail)throw new Error('send failure')}};
 let error=null;try{socket.ping()}catch(e){error={name:e.name,message:e.message}}
 globalThis.result={input,sent,logs,reconnects,pingAt:socket.pingAt,error};`,{input,WebSocket:{OPEN:1},LogSeverity:{Debug:'debug',Warning:'warning'},performance:{now:()=>123},isWsDiagnosticsEnabled:()=>{diagnosticChecks++;return diagnostics}});
 cases.push({...result,diagnosticChecks});
}
const timestampBody='class Witness {\n'+members(venue,['parseTimestampToSeconds'])+'\n}\nglobalThis.witness=new Witness();';
const context={};execute(timestampBody,context);
function encode(value){if(value===undefined)return {$undefined:true};if(typeof value==='number'&&(!Number.isFinite(value)||Object.is(value,-0)))return {$number:Object.is(value,-0)?'-0':String(value)};return value}
const inputs=[undefined,null,false,true,0,-0,NaN,Infinity,-Infinity,'',' ','\ufeff','\u0085','0x10','0b11','0o17','12abc','1e400','-1e-400',[],[2],[1,2],{}, {toString:null},{valueOf:1}];
for(const boundary of [0,1,1000,1e12,1e15])for(const delta of [-1,-0.1,0,0.1,1])for(const sign of [1,-1])for(const shape of ['number','string']){const value=(boundary+delta)*sign;inputs.push(shape==='number'?value:String(value))}
const timestamps=inputs.map(value=>{try{return {input:encode(value),ok:encode(context.witness.parseTimestampToSeconds(value))}}catch(e){return {input:encode(value),error:{name:e.name,message:e.message}}}});
for(const [name,rows] of [['private-heartbeats',cases],['private-timestamps',timestamps]]){
 const file=path.join(__dirname,'../tests/fixtures/'+name+'.json'),bytes=JSON.stringify(rows,null,2)+'\n';
 if(process.argv.includes('--check'))assert.equal(fs.readFileSync(file,'utf8'),bytes,name+' actual source drift');else fs.writeFileSync(file,bytes);
 console.log('WOO '+name+' cases:',rows.length);
}
