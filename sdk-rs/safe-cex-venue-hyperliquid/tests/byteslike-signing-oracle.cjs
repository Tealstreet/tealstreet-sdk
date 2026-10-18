'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const root = path.resolve(__dirname, '../../..');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || require.resolve('typescript', {paths:[root]}));
const {ethers} = require('ethers'), {encode} = require('@msgpack/msgpack');
const tree = ts.createSourceFile('hyperliquid.signing.ts', fs.readFileSync(path.join(root,'packages/safe-cex/src/exchanges/hyperliquid/hyperliquid.signing.ts'),'utf8'),ts.ScriptTarget.Latest,true);
assert.equal(tree.parseDiagnostics.length,0);
const names = ['addressToBytes','actionHash','constructPhantomAgent','signL1Action','signInner','splitSig'];
const declarations = names.map(name => {
  const found = tree.statements.filter(node => ts.isFunctionDeclaration(node) && node.name?.text === name);
  assert.equal(found.length,1,name); return found[0].getText(tree);
});
for (const name of ['phantomDomain','agentTypes']) {
  const found = tree.statements.filter(node => ts.isVariableStatement(node) && node.declarationList.declarations.some(d => d.name.getText(tree) === name));
  assert.equal(found.length,1,name); declarations.push(found[0].getText(tree));
}
const context = {exports:{},ethers,encode,Uint8Array,DataView};
vm.runInNewContext(ts.transpileModule(declarations.join('\n')+'\nObject.assign(exports,{addressToBytes,actionHash,signL1Action});',{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText,context);
(async () => {
  const wallet = new ethers.Wallet('0x'+'11'.repeat(32));
  const action = {type:'cancel',cancels:[{a:1,o:1234}]};
  const rows = [];
  for (const vault of ['0x12','0X12','0x','0X','0xAB','0xzz','0x1','12']) {
    try {
      rows.push({vault,ok:true,bytes:Array.from(context.exports.addressToBytes(vault)),hash:context.exports.actionHash(action,vault,1700000000123),signature:await context.exports.signL1Action(wallet,action,vault,1700000000123,true)});
    } catch (e) {
      rows.push({vault,ok:false,error:Object.fromEntries(['name','message','code','argument','value','shortMessage'].map(k=>[k,e[k]]))});
    }
  }
  assert.equal(rows.filter(r=>r.ok).length,5);
  assert.equal(rows.filter(r=>!r.ok).length,3);
  process.stdout.write(JSON.stringify(rows)+'\n');
})().catch(error=>{process.stderr.write(error.stack+'\n');process.exitCode=1;});
