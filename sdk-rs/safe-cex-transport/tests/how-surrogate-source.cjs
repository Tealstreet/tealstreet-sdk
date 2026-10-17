const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const root=path.resolve(__dirname,'../../..');
function moduleCode(file){
 const source=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
 const text=source.statements.filter(n=>!ts.isImportDeclaration(n)).map(n=>n.getText(source)).join('\n');
 return ts.transpileModule(text,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
}
const parsing={};
new Function('exports',moduleCode(path.join(root,'packages/json-parse/src/jsonParse.ts')))(parsing);
const binary={};
new Function('exports','jsonParse',moduleCode(path.join(root,'packages/http-over-websocket/src/tradeTunnelBinaryFrame.ts')))(binary,parsing.jsonParse);
const file=path.join(root,'packages/http-over-websocket/src/HOWWebSocket.ts');
const ast=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
const owner=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text==='HOWWebSocket');
const field=owner.members.find(n=>ts.isPropertyDeclaration(n)&&n.name.getText(ast)==='onMessage');
assert(field?.initializer);
const code=ts.transpileModule(`class Source {onMessage=${field.initializer.getText(ast)};}`,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText;
const Source=new Function('decodeTradeTunnelMessage',code+';return Source;')(binary.decodeTradeTunnelMessage);
const body='{"label":"tp \\ud800","\\udfff":"kept","list":[1,2]}';
const frame='{"type":"rpc.http","nonce":"witness","data":{"status":200,"headers":{},"data":'+body+'}}';
const units=s=>Array.from({length:s.length},(_,i)=>s.charCodeAt(i));
function inspect(value){return {label:units(value.label),keys:Object.keys(value).map(units),rows:value.list.length};}
(async()=>{
 const socket=new Source();socket.isDisposed=false;
 const emitted=[];socket.emit=(name,value)=>emitted.push({name,value});
 await socket.onMessage({data:frame});
 assert.equal(emitted.length,1);assert.equal(emitted[0].name,'sub:witness');
 const ordinary=inspect(emitted[0].value.data),raw=inspect(parsing.jsonParse(body));
 assert.deepEqual(ordinary.label,[116,112,32,0xd800]);
 assert.deepEqual(ordinary.keys[1],[0xdfff]);assert.equal(ordinary.rows,2);
 assert.deepEqual(raw,ordinary);
 console.log(JSON.stringify({body,ordinary,raw,delivered:emitted.length}));
})().catch(error=>{console.error(error);process.exitCode=1;});
