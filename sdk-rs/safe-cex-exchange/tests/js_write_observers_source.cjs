'use strict';
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const file = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/base.ts');
const source = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
const base = source.statements.find(n => ts.isClassDeclaration(n) && n.name?.text === 'BaseExchange');
const ctor = base?.members.find(ts.isConstructorDeclaration);
if (!ctor) throw Error('maintained BaseExchange constructor missing');
const writes = [['applicationId', 'master'], ['vault', 'vault'], ['applicationId', ''],
  ['vault', undefined], ['applicationId', '账户😀'], ['applicationId', undefined]];
const context = {normalizeSymbols: value => value, writes, result: undefined};
const text = `'use strict';
class SourceConstructor {
 emitter = {on(){}, once(){}, off(){}, emit(){}};
 ${ctor.getText(source)}
}
const opts = {key:'key',secret:'secret'};
const instance = new SourceConstructor('hyperliquid', opts, {subscribe(){return ()=>{}}});
const alias = instance.options;
const identity = [];
for (const [key,value] of writes) {
 alias[key] = value;
 identity.push([instance.options.applicationId ?? null, instance.options.vault ?? null]);
}
Object.freeze(opts);
let failure;
try { alias.applicationId = 'refused'; } catch(e) { failure=e.name; }
result = {sameObject:alias===opts, identity, frozenError:failure,
 final:[instance.options.applicationId ?? null, instance.options.vault ?? null]};`;
vm.runInNewContext(ts.transpileModule(text, {compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText, context);
const trace = [];
const target = Object.create({set inherited(v) { this.inner = v; }});
let proxy;
proxy = new Proxy(target, {set(t,k,v,r) {
 if (!Reflect.set(t,k,v,r)) return false;
 trace.push([k, r===proxy, r[k] ?? null]);
 return true;
}});
proxy.inherited = 9;
const cause = {};
const failed = new Proxy({}, {set(t,k,v,r) { Reflect.set(t,k,v,r); throw cause; }});
let sameCause = false;
try { failed.value=4; } catch(e) { sameCause=e===cause; }
console.log(JSON.stringify({...context.result, trace, failedWrite:failed.value, sameCause}));
