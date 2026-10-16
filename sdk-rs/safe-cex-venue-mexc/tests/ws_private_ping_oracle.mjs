import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {root,ts} from './source.mjs';
const filename='packages/safe-cex/src/exchanges/mexc/mexc.ws-private.ts';
const source=fs.readFileSync(`${root}/${filename}`,'utf8');
const ast=ts.createSourceFile(filename,source,ts.ScriptTarget.Latest,true);
let initializer;
const find=node=>{
  if(ts.isPropertyDeclaration(node) && node.name.getText(ast)==='ping') initializer=node.initializer;
  ts.forEachChild(node,find);
};
find(ast);
assert(initializer,'MexcPrivateWebsocket#ping was not found');
let functionText=initializer.getText(ast);
if(process.env.MEXC_WS_PRIVATE_PING_CONTROL==='wrong-method') {
  const changed=functionText.replace("method: 'ping'", "method: 'pong'");
  assert.notEqual(changed,functionText,'named in-memory private ping mutation did not apply');
  functionText=changed;
}
const js=ts.transpileModule(`class Receiver { constructor(){this.sent=[];} sendWithDiagnostics=(data)=>this.sent.push(data); ping=${functionText}; } exports.create=()=>new Receiver();`,{
  compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS},
}).outputText;
const module={exports:{}};
vm.runInNewContext(`(function(exports){${js}\n})(module.exports)`,{module,performance:{now:()=>123}});
const receiver=module.exports.create();
receiver.ping();
assert.equal(receiver.sent.length,1);
process.stdout.write(JSON.stringify(JSON.parse(receiver.sent[0])));
