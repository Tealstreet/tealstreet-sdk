const fs=require('node:fs'),assert=require('node:assert/strict');
const {root,ts}=require('../../safe-cex-types/scripts/accounts-source.cjs');
const source=fs.readFileSync(root+'/packages/http-over-websocket/src/HOWWebSocket.ts','utf8');
const ast=ts.createSourceFile('how.ts',source,ts.ScriptTarget.Latest,true);
const klass=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text==='HOWWebSocket');
const field=klass.members.find(n=>n.name?.getText(ast)==='onMessage');
assert.ok(field?.initializer,'actual source onMessage initializer');
const js=ts.transpileModule('const onMessage='+field.initializer.getText(ast)+';',{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText;
let decodes=0;const times=[],owner={isDisposed:false};
const onMessage=new Function('decodeTradeTunnelMessage','syncServerTime',js+';return onMessage;').call(owner,async data=>{decodes++;return JSON.parse(data)},time=>times.push(time));
(async()=>{
 await onMessage({data:'{"type":"serverTime","serverTime":1000}'});
 assert.deepEqual(times,[1000],'source active positive control');
 owner.isDisposed=true;
 await onMessage({data:'{"type":"serverTime","serverTime":1000}'});
 assert.equal(decodes,1,'source disposed guard before decode');
 assert.equal(times.length,1,'source disposed guard before callback');
})().catch(error=>{console.error(error);process.exitCode=1});
