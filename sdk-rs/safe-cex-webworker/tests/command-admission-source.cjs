const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const root=path.resolve(__dirname,'../../..');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH || require.resolve('typescript'));
const source=fs.readFileSync(path.join(root,'packages/safe-cex/src/webworker/handlers/CommandHandler.ts'),'utf8');
const js=ts.transpileModule(source,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
const moduleObject={exports:{}};let calls=0;const events=[];
vm.runInNewContext(js,{module:moduleObject,exports:moduleObject.exports,require(name){
 if(name==='../commandExecutor')return {getCommandExecutor:()=>async()=>{calls++;}};
 if(name==='./BaseHandler')return {BaseHandler:class {constructor(exchange){this._exchange=exchange;}}};
 throw Error(name);
}});
const handler=new moduleObject.exports.CommandHandler({emitter:{emit:(name,data)=>events.push([name,data])}});
for(const data of [undefined,null,false,true,0,1,'','buy 1']) {
 events.length=0;handler.handle({op:'exec',data});
 assert.equal(calls,0);assert.deepEqual(JSON.parse(JSON.stringify(events)),[['exec-log',{message:'Invalid command payload.'}]]);
}
handler.handle({op:'exec',data:[]});assert.equal(calls,1);
console.log('Actual CommandHandler source: eight invalid payloads refused; arrays retain object admission PASS');
