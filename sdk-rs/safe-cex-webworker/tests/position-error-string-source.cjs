const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const root=path.resolve(__dirname,'../../..');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH || require.resolve('typescript'));
const text=fs.readFileSync(path.join(root,'packages/safe-cex/src/webworker/handlers/UpdatePositionsHandler.ts'),'utf8');
const js=ts.transpileModule(text,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS,esModuleInterop:true}}).outputText;
const m={exports:{}};
vm.runInNewContext(js,{module:m,exports:m.exports,console:{error(){}},require(name){
 if(name==='lodash/isEqual')return ()=>{throw Error('equality must not run after rejection');};
 if(name==='./BaseHandler')return {BaseHandler:class {constructor(exchange,ctx){this._exchange=exchange;this._ctx=ctx;}}};
 throw Error(name);
}});
(async()=>{
for(const [name,message,expected] of [['Error','','Error'],['','', ''],['','failed','failed'],['TypeError','failed','TypeError: failed']]) {
 const error=new Error(message);error.name=name;const messages=[];
 const handler=new m.exports.UpdatePositionsHandler({async fetchPositions(){throw error;}},{postMessage(value){messages.push(value);}});
 await handler.handleUpdatePositions('string-error');
 assert.deepEqual(JSON.parse(JSON.stringify(messages)),[{op:'updatePositions:done',data:{requestId:'string-error',success:false,reason:expected}}]);
}
console.log('Actual UpdatePositionsHandler source: four real Error name/message projections PASS');
})().catch(error=>{console.error(error);process.exitCode=1;});
