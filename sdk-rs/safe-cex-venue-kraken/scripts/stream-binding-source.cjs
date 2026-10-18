// Execute the unchanged live Futures socket candle members, selected by TS AST.
'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
assert.ok(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const file = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/kraken/kraken.ws-public.ts');
const source = ts.createSourceFile(file, fs.readFileSync(file,'utf8'), ts.ScriptTarget.Latest, true);
const cls = source.statements.filter(n=>ts.isClassDeclaration(n)&&n.name.text==='KrakenPublicWebsocket');
assert.equal(cls.length,1);
function member(name) {
 const nodes=cls[0].members.filter(n=>n.name?.getText(source)===name); assert.equal(nodes.length,1);
 const result=ts.transpileModule(`(${nodes[0].initializer.getText(source)});`,{compilerOptions:{target:ts.ScriptTarget.ES2022},reportDiagnostics:true});
 assert.equal((result.diagnostics||[]).filter(d=>d.category===ts.DiagnosticCategory.Error).length,0);
 return vm.runInNewContext(result.outputText);
}
const listen=member('listenOHLCV'), handle=member('handleOHLC');
let calls=0; const callback=()=>{calls++;};
const inputs=[undefined,null,false,-0,NaN,'\ud800',[],{}];
for(const input of inputs){
 const first=listen(input,callback),second=listen(input,callback);
 assert.notEqual(first,second); assert.equal(first(),undefined); assert.equal(first(),undefined); assert.equal(second(),undefined);
 assert.equal(handle(input),undefined);
}
assert.equal(calls,0);
console.log(JSON.stringify(inputs.length));
