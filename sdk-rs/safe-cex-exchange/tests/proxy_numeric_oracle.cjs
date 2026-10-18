const fs = require('node:fs');
const path = require('node:path');
const assert = require('node:assert/strict');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const file = path.resolve(__dirname, '../../../packages/safe-cex/src/proxyTokenClient.ts');
const ast = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
const source = ast.statements.filter(n => !ts.isImportDeclaration(n) && !(ts.isExportDeclaration(n) && n.moduleSpecifier)).map(n => n.getText(ast)).join('\n') + '\nexport {initBroadcastChannel}; export const snapshot = () => cachedExpiresAt;';
let channel;
class Channel {
  constructor(name) { assert.equal(name, 'proxy-token-channel'); channel = this; }
}
const exportsObject = {};
new Function('exports', 'BroadcastChannel', ts.transpileModule(source, {compilerOptions: {target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS}}).outputText)(exportsObject, Channel);
exportsObject.initBroadcastChannel();
assert.equal(typeof channel.onmessage, 'function', 'actual source installs broadcast receiver');
let ignored = null; for (let i = 0; i < 160; i++) ignored = [ignored];
channel.onmessage({data:{type:'proxyTokenUpdate',token:'fixture',expiresAt:1,ignored}});
assert.equal(exportsObject.snapshot(),1,'actual receiver accepts 160-deep ignored source metadata');
if (process.argv.includes('--utf16')) {
  const seen=[];const stop=exportsObject.onTokenUpdate(token=>seen.push(token));
  const rows=['\ud800','x\udc00'].map(token=>{
    const input={type:'proxyTokenUpdate',token,expiresAt:1};channel.onmessage({data:input});
    const cached=exportsObject.getCurrentToken(),callback=seen.at(-1);
    assert.equal(cached,token,'actual receiver/getCurrentToken retains native token BEFORE serialization');
    assert.equal(callback,token,'actual callback retains native token BEFORE serialization');
    const units=Array.from({length:cached.length},(_,i)=>cached.charCodeAt(i));
    assert.deepEqual(units,token.length===1?[0xd800]:[0x78,0xdc00]);
    return {units,escaped:JSON.stringify(input)};
  });
  const scalar='😀';channel.onmessage({data:{type:'proxyTokenUpdate',token:scalar,expiresAt:1}});
  assert.equal(exportsObject.getCurrentToken(),scalar);assert.equal(seen.at(-1),scalar);
  assert.equal(seen.length,3,'two native UTF16 token shapes plus scalar source positive');stop();
  process.stdout.write(JSON.stringify({rows,scalar}));process.exit(0);
}
const results = process.argv.slice(2).map(lexeme => {
  channel.onmessage({data: JSON.parse(`{"type":"proxyTokenUpdate","token":"fixture","expiresAt":${lexeme}}`)});
  const native = exportsObject.snapshot();
  assert.equal(typeof native, 'number', 'native cached expiry BEFORE wire projection');
  const buffer = Buffer.alloc(8); buffer.writeDoubleBE(native); return buffer.readBigUInt64BE().toString();
});
assert.equal(results.length, 11, 'independent numeric spelling floor');
process.stdout.write(JSON.stringify(results));
