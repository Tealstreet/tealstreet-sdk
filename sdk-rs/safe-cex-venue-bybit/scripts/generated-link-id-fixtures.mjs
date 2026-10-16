import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import vm from 'node:vm';
import ts from 'typescript';
const source=ts.createSourceFile('bybit.ts',readFileSync(new URL('../../../packages/safe-cex/src/exchanges/bybit/bybit.exchange.ts',import.meta.url),'utf8'),ts.ScriptTarget.Latest,true);
const owner=source.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text==='BybitExchange');
const members=['requiresOrderLinkId','generatedOrderLinkId'].map(name=>{
  const selected=owner.members.filter(n=>n.name?.getText(source)===name);
  assert.equal(selected.length,1,`LINK-ID source member ${name}`);
  return selected[0].getText(source);
});
let generatorCalls=0;
const subject=vm.runInNewContext(ts.transpileModule(`new class { ${members.join('\n')} }`,{}).outputText,
  {nanoid:()=>{generatorCalls++;throw Error('linear generation must be unreachable');}});
assert.equal(subject.requiresOrderLinkId(),false,'LINK-ID source hook');
const results=[];
for(let i=0;i<3;i++) results.push(typeof subject.generatedOrderLinkId());
assert.deepEqual(results,['undefined','undefined','undefined'],'LINK-ID source results');
assert.equal(generatorCalls,0,'LINK-ID source generator calls');
process.stdout.write(JSON.stringify({capture:{architecture:process.arch,node:process.versions.node},results,generatorCalls}));
