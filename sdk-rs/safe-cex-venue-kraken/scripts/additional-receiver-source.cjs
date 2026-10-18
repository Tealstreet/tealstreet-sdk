'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const crypto = require('node:crypto');
assert.ok(process.env.SAFE_CEX_TYPESCRIPT_PATH, 'SAFE_CEX_TYPESCRIPT_PATH is required');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const root = path.resolve(__dirname, '../../..');
const file = path.join(root, 'packages/safe-cex/src/exchanges/kraken/krakenspot.exchange.ts');
const source = fs.readFileSync(file, 'utf8');
const ast = ts.createSourceFile(file, source, ts.ScriptTarget.Latest, true);
const cls = ast.statements.find(s => ts.isClassDeclaration(s) && s.name.text === 'KrakenSpotExchange');
const members = cls.members.filter(m => m.name?.getText(ast) === 'cancelOrders');
assert.equal(members.length, 1);
assert.ok(ts.isPropertyDeclaration(members[0]) && ts.isArrowFunction(members[0].initializer));
const typesFile = path.join(path.dirname(file), 'krakenspot.types.ts');
const typesSource = fs.readFileSync(typesFile, 'utf8');
const typesAst = ts.createSourceFile(typesFile, typesSource, ts.ScriptTarget.Latest, true);
const endpoints = typesAst.statements.flatMap(s => ts.isVariableStatement(s) ? [...s.declarationList.declarations] : []).find(d => d.name.getText(typesAst) === 'ENDPOINTS');
function compile(text) {
  const result = ts.transpileModule(text, {compilerOptions:{target:ts.ScriptTarget.ES2022},reportDiagnostics:true});
  assert.equal((result.diagnostics||[]).filter(d=>d.category===ts.DiagnosticCategory.Error).length,0);
  return result.outputText;
}
const capture = vm.runInNewContext(compile(`const ENDPOINTS=${endpoints.initializer.getText(typesAst)}; function capture(){return (${members[0].initializer.getText(ast)});}`)+';capture', {}, {filename:file});
(async()=>{
  const messages=['\ud800X\udc00','\udc00',Infinity,7,true,undefined,null,false,0,-0,NaN,''];
  for(const message of messages){
    const original={message}; const emitted=[]; let gets=0;
    const orders=[Object.defineProperty({},'id',{get(){gets++;throw original;}})];
    const cancel=capture.call({emitAndLogError(...args){emitted.push(args);},unlimitedXHR:{post(){assert.fail('id must throw first');}}});
    const selected=message||'Failed to cancel orders';
    await assert.rejects(cancel(orders),error=>error!==original&&error.name==='Error'&&error.message===String(selected));
    assert.equal(gets,1); assert.equal(emitted.length,1);assert.equal(emitted[0][0],selected);assert.equal(emitted[0][1],'CancelOrders Error');
  }
  console.log(JSON.stringify({sourceSha256:crypto.createHash('sha256').update(source).digest('hex'),profiles:messages.length,originalSelectedValue:true,emitBeforeFreshError:true}));
})().catch(error=>{console.error(error);process.exitCode=1;});
