const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const root = path.resolve(__dirname, '../../..');
const source = ts.createSourceFile('hyrotrader.ws-private.ts', fs.readFileSync(path.join(root, 'packages/safe-cex/src/exchanges/bybit/hyrotrader.ws-private.ts'), 'utf8'), ts.ScriptTarget.Latest, true);
assert.equal(source.parseDiagnostics.length, 0);
const owner = source.statements.find(node => ts.isClassDeclaration(node) && node.name.text === 'HyroPrivateWebsocket');
const methods = owner.members.filter(node => ts.isMethodDeclaration(node) && node.name.getText(source) === 'resetSession');
assert.equal(methods.length, 1);
const compiled = ts.transpileModule(`class Source { ${methods[0].getText(source)} }`, {compilerOptions:{target:ts.ScriptTarget.ES2022}, reportDiagnostics:true});
assert.equal(compiled.diagnostics.length, 0);
const traces = [];
for (const liveAddition of [false, true]) {
  const trace = [];
  const Source = new Function('clearTimeout', `${compiled.outputText};return Source;`)(timer => trace.push(`clear:${timer}`));
  const owner = new Source();
  Object.assign(owner, {generation:4,stateRevision:9,ready:true,lastReceived:77,pendingAcks:new Map(),parent:{syncSocketHealthToStore(){trace.push(`health:${owner.lastReceived}`);}}});
  const pending = (label,timer,late=false) => ({timer,reject(error){trace.push(`reject:${label}:${error.message}`);if(late)owner.pendingAcks.set('late',pending('late',33));}});
  owner.pendingAcks.set('auth',pending('auth',11,liveAddition));
  owner.pendingAcks.set('subscribe',pending('subscribe',22));
  owner.resetSession();
  assert.equal(owner.generation,5);assert.equal(owner.stateRevision,10);assert.equal(owner.ready,false);assert(Object.is(owner.lastReceived,0));assert.equal(owner.pendingAcks.size,0);
  traces.push(trace);
}
process.stdout.write(JSON.stringify(traces));
