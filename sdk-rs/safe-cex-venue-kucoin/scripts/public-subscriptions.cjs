const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const {root}=require('./tables.cjs');
const file=path.join(root,'packages/safe-cex/src/exchanges/kucoin/kucoin.ws-public.ts');
const ast=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
const cls=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text==='KucoinPublicWebsocket');
const names=['subscribe','subscribeActiveTicker','unsubscribeActiveTicker'];
const members=names.map(name=>{const m=cls.members.find(n=>n.name?.getText(ast)===name);assert.ok(m,'maintained '+name);return m.getText(ast);});
const constants=['SNAPSHOT_CHANNEL','ACTIVE_DETAIL_TICKER_CHANNELS'].map(name=>{
 const statement=ast.statements.find(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>d.name.getText(ast)===name));
 assert.ok(statement,'maintained '+name);return statement.getText(ast);
});
const code=ts.transpileModule(constants.join('\n')+'\nexport class Selected {'+members.join('\n')+'}',{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
const moduleObject={exports:{}};new Function('module','exports',code)(moduleObject,moduleObject.exports);
const input=JSON.parse(fs.readFileSync(0,'utf8'));assert.equal(input.active.length,32);assert.equal(input.snapshot.length,18);
const text=units=>String.fromCharCode(...units),units=text=>Array.from({length:text.length},(_,i)=>text.charCodeAt(i));
const capture=(operation,channels,symbols)=>({operation,channels,symbols:symbols.map(units)});
const active=[];
for(const c of input.active){
 const receiver=new moduleObject.exports.Selected(),calls=[],failure=new TypeError('subscription failure');
 receiver.isDisposed=c.disposed;receiver.sendChannelOp=(...args)=>{calls.push(capture(...args));if(c.fail)throw failure;};let error=null;
 try{receiver[c.subscribe?'subscribeActiveTicker':'unsubscribeActiveTicker'](text(c.symbol));}
 catch(e){assert.equal(e,failure);error={name:e.name,message:e.message,same:e===failure};}
 active.push({calls,error});
}
const snapshot=[];
for(const c of input.snapshot){
 const receiver=new moduleObject.exports.Selected(),calls=[],trace=[],failure=new TypeError('subscription failure');
 receiver.role=c.role;receiver.isDisposed=true;
 receiver.snapshotShardSymbols=()=>{trace.push('shard');if(c.failure==='shard')throw failure;return c.symbols.map(text);};
 receiver.sendChannelOp=(...args)=>{trace.push('send');calls.push(capture(...args));if(c.failure==='send')throw failure;};let error=null;
 try{receiver.subscribe();}catch(e){assert.equal(e,failure);error={name:e.name,message:e.message,same:e===failure};}
 snapshot.push({trace,calls,error});
}
process.stdout.write(JSON.stringify({active,snapshot}));
