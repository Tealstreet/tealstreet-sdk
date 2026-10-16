// Execute the source constructor body; host emitter/store boundaries are inert.
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const file = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/base.ts');
const source = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
const base = source.statements.find(node => ts.isClassDeclaration(node) && node.name?.text === 'BaseExchange');
const ctor = base?.members.find(ts.isConstructorDeclaration);
if (!ctor) throw Error('source BaseExchange constructor missing');
const text = `class SourceConstructor {
 emitter = {on(){}, once(){}, off(){}, emit(){}};
 ${ctor.getText(source)}
}
const opts = {key:'key', secret:'secret'};
const instance = new SourceConstructor('hyperliquid', opts, {subscribe(){return ()=>{}}});
const retained = instance.options;
instance.options.applicationId = 'master';
result = {sameObject: retained === opts, retainedIdentity: retained.applicationId,
 currentIdentity: instance.options.applicationId};`;
const context = {normalizeSymbols: value => value, result: undefined};
vm.runInNewContext(ts.transpileModule(text, {compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText, context);
console.log(JSON.stringify(context.result));
