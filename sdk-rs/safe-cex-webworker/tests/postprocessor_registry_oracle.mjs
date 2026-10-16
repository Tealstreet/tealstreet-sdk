import fs from 'node:fs';
import vm from 'node:vm';
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
const require=createRequire(import.meta.url),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const path=new URL('../../../packages/safe-cex/src/webworker/postProcessors/registry.ts',import.meta.url);
const ast=ts.createSourceFile(path.pathname,fs.readFileSync(path,'utf8'),ts.ScriptTarget.Latest,true);
const independent=['DisplayedPositionsPostprocessor','DisplayedMarginPostprocessor','ChaserOrderEnrichmentPostprocessor','OrdersRiskProfilePostprocessor'];
const dependent=['ChaserOrderPostprocessor','TwapPostprocessor','MessageHandlerStatsPostprocessor'];
// Required imported constructor identities, supplied by the fixture loader.
const ctx=vm.createContext({});
for(const name of [...independent,...dependent]) ctx[name]=class {constructor(value){this.value=value;this.name=name;}};
const source=ast.statements.filter(n=>!ts.isImportDeclaration(n)).map(n=>n.getText(ast)).join('\n').replace(/\bexport /g,'');
vm.runInContext(ts.transpileModule(source+'\nglobalThis.lists=[independentPostprocessors,exchangeDependentPostprocessors];globalThis.register=registerIndependentPostprocessor;', {compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,ctx);
const [list,other]=ctx.lists;
const labels=new Map([...independent,...dependent].map(name=>[ctx[name],name]));
const extension=class {constructor(value){this.value=value;this.name='extension';}};
// Same display name, distinct function identity: both must remain.
const twin=class {constructor(value){this.value=value;this.name='extension';}};
labels.set(extension,'extension');labels.set(twin,'twin');
const rows=[];
function snapshot(label){rows.push({label,independent:Array.from(list,c=>labels.get(c)),dependent:Array.from(other,c=>labels.get(c)),constructed:Array.from(list,c=>{const x=new c(1.25);return [x.name,x.value];})});}
assert.deepEqual(Array.from(list,c=>labels.get(c)),independent,'REGISTRY source initial independent');
assert.deepEqual(Array.from(other,c=>labels.get(c)),dependent,'REGISTRY source initial dependent');
snapshot('initial');
assert.equal(ctx.register(ctx.DisplayedMarginPostprocessor),undefined);snapshot('builtin-duplicate');
assert.equal(ctx.register(extension),undefined);snapshot('append');
ctx.register(extension);snapshot('same-extension');
ctx.register(twin);snapshot('same-name-distinct');
const held=list;list.splice(1,1);snapshot('external-remove');
ctx.register(ctx.DisplayedMarginPostprocessor);snapshot('reappend-builtin');
list.reverse();ctx.register(extension);snapshot('external-reorder-and-duplicate');
other.reverse();snapshot('dependent-reorder');
assert.equal(held,ctx.lists[0],'REGISTRY source list identity');
assert.equal(rows.length,9,'REGISTRY fixed operation floor');
process.stdout.write(JSON.stringify({rows,listIdentity:true}));
