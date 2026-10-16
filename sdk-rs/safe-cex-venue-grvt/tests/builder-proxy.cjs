'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const compiler = process.env.SAFE_CEX_TYPESCRIPT_PATH;
assert(compiler && path.isAbsolute(compiler), 'absolute isolated compiler required');
const ts = require(compiler);
function declarations(file, names) {
  const text = fs.readFileSync(file, 'utf8');
  const tree = ts.createSourceFile(file, text, ts.ScriptTarget.Latest, true);
  return names.map(name => {
    const found = tree.statements.filter(s => ts.isVariableStatement(s) && s.declarationList.declarations.some(d => d.name.getText(tree) === name));
    assert.equal(found.length, 1, `BUILDER-PROXY source declaration ${name}`);
    return found[0].getText(tree);
  }).join('\n');
}
const dir = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/grvt');
const names = ['grvtProxy','viaGrvtProxy','resolveGrvtProxyHeaders','getGrvtEdgeApiUrl','getGrvtTradingApiUrl'];
let source = declarations(path.join(dir,'grvt.types.ts'), ['BASE_URLS']) + '\n' + declarations(path.join(dir,'grvt.builder.ts'), names);
if (process.env.GRVT_PROXY_MUTATION === 'empty') {
  assert(source.includes('return proxy ?'), 'proxy source mutation match');
  source = source.replace('return proxy ?', 'return proxy?.url ?');
} else assert(!process.env.GRVT_PROXY_MUTATION, 'unknown source mutation');
const compiled = ts.transpileModule(source + '\nObject.assign(exports, {'+names.join(',')+'});', {compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS},reportDiagnostics:true});
assert.equal(compiled.diagnostics.length, 0, 'BUILDER-PROXY transpile');
(async () => {
  const rows=[];
  for (const proxyUrl of [null,'','https://proxy','https://proxy/','代理/😀']) {
    for (const testnet of [false,true]) {
      const effects=[];
      const proxy = proxyUrl === null ? undefined : {url:proxyUrl, resolveHeaders:async()=>{effects.push('headers');return {Zulu:'last',Alpha:'first'};}};
      const context={exports:{},getTransport:opts=>{assert.deepEqual(Object.keys(opts),[]);effects.push('get');return {privateProxy:exchange=>{effects.push(exchange);return proxy;}};}};
      vm.runInNewContext(compiled.outputText,context);
      const api=context.exports;
      const same=api.grvtProxy()===proxy;
      const via=api.viaGrvtProxy('base/?q=1');
      const edge=api.getGrvtEdgeApiUrl(testnet);
      const trading=api.getGrvtTradingApiUrl(testnet);
      const pending=api.resolveGrvtProxyHeaders();
      const atInvocation=[...effects];
      const headers=await pending;
      rows.push({proxyUrl,testnet,same,via,edge,trading,headers,headerOrder:Object.keys(headers),effects,atInvocation});
    }
  }
  const failureEffects=[];
  const failure = new Error('credential failure');
  const failureProxy={url:'',resolveHeaders:()=>{failureEffects.push('headers');return Promise.reject(failure);}};
  let selected=failureProxy;
  const context={exports:{},getTransport:()=>{failureEffects.push('get');return {privateProxy:e=>{failureEffects.push(e);return selected;}};}};
  vm.runInNewContext(compiled.outputText,context);
  const pending=context.exports.resolveGrvtProxyHeaders();
  const failureAtInvocation=[...failureEffects];
  selected=undefined;
  let rejectedSame=false;
  try { await pending; } catch(error) { rejectedSame=error===failure; }
  assert(rejectedSame,'BUILDER-PROXY source rejection identity');
  assert.deepEqual(failureAtInvocation,['get','grvt','headers'],'BUILDER-PROXY source invocation timing');
  const abandonedEffects=[];let release;
  const deferredProxy={resolveHeaders:()=>{abandonedEffects.push('headers');return new Promise(resolve=>{release=resolve}).then(()=>{abandonedEffects.push('continued');return {Zulu:'last',Alpha:'first'}})}};
  const deferredContext={exports:{},getTransport:()=>({privateProxy:()=>deferredProxy})};
  vm.runInNewContext(compiled.outputText,deferredContext);
  void deferredContext.exports.resolveGrvtProxyHeaders();release();
  await Promise.resolve();await Promise.resolve();await Promise.resolve();
  assert.deepEqual(abandonedEffects,['headers','continued'],'BUILDER-PROXY source discarded observer continuation');
  process.stdout.write(JSON.stringify({architecture:process.arch,node:process.versions.node,rows,failureAtInvocation,rejectedSame,abandonedEffects}));
})().catch(error=>{console.error(error);process.exitCode=1;});
