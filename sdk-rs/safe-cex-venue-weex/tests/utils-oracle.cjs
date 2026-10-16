// One process executes the complete seven-normalizer corpus from original AST nodes.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const {load, root, ts, header, capture} = require('./types-oracle.cjs');
const rows = require('./utility-enums.json');
const file = path.join(root, 'packages/safe-cex/src/exchanges/weex/weex.utils.ts');
const source = fs.readFileSync(file, 'utf8');
const ast = ts.createSourceFile(file, source, ts.ScriptTarget.Latest, true);
const wanted = new Set(rows.map(r => r[0]));
const selected = ast.statements.filter(n => ts.isVariableStatement(n) && n.declarationList.declarations.some(d => wanted.has(d.name.getText(ast))));
assert.equal(selected.length, wanted.size, 'utility_source_inventory');
// Use verbatim declarations, including private normalizeTriggerType; only exports appended.
const text = selected.map(n => n.getText(ast)).join('\n') + '\nexport { normalizeTriggerType };';
const enums = load(path.join(root, 'packages/safe-cex/src/types.ts'));
const tables = load(path.join(root, 'packages/safe-cex/src/exchanges/weex/weex.types.ts'));
const context = {...enums, ...tables, exports: {}};
vm.runInNewContext(ts.transpileModule(text, {compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText, context, {filename:file});
const raws = new Set(['null','false','true','0','-0','-0.0','-1e-4000','1e400','-1e400','9007199254740993.0','1','""','"unknown"','"ß"','"ısolated"','"ſhort"','"constructor"','"__proto__"','"toString"','"valueOf"','"hasOwnProperty"','{}','[]','[null]','[[]]','{"toString":null}','[{"toString":0}]','{"valueOf":null}', '"𝒜"']);
for(const [, , , table] of rows) for(const key of Object.keys(tables[table])) {
  for(const v of [key,key.toLowerCase(),` ${key} `,[key],[[key]],[key,null]]) raws.add(JSON.stringify(v));
}
const cases = [{id:'undefined', absent:true}, ...Array.from(raws,(raw,i)=>({id:`json_${i}`,raw}))];
const outcomes = [];
for(const row of rows) for(const c of cases) {
  try { const native=context.exports[row[0]](c.absent ? undefined : JSON.parse(c.raw)); outcomes.push({...c,fn:row[0],result:native === undefined ? {kind:'undefined'} : {kind:'enum',value:native}}); }
  catch(e) { outcomes.push({...c,fn:row[0],result:{kind:'error',name:e.name,message:e.message}}); }
}
// JSON escapes accepted in JS but refused at the shared Rust UTF-8 boundary.
const refusals = rows.map(row => ({fn:row[0],raw:'"\\ud800"',native:context.exports[row[0]]('\ud800') ?? null}));
// Dynamic callable coercion is outside the retained JSON contract, and can change a result.
const callable = rows.map(row => { const key=Object.keys(tables[row[3]]).find(k=>tables[row[3]][k] !== (row[4] ? enums[row[2]][row[4]] : undefined)); return {fn:row[0],native:context.exports[row[0]]({toString:()=>key}),plain:context.exports[row[0]]({})??null}; });
for(const c of callable) assert.notEqual(c.native,c.plain,'callable_coercion_supplement');
const depth = rows.map(row => { const key=Object.keys(tables[row[3]]).find(k=>tables[row[3]][k] !== (row[4] ? enums[row[2]][row[4]] : undefined)); const raw='['.repeat(150)+JSON.stringify(key)+']'.repeat(150); return {fn:row[0],raw,native:context.exports[row[0]](JSON.parse(raw))}; });
for(let i=0;i<depth.length;i++) assert.equal(depth[i].native,callable[i].native,'deep_array_source_coercion');
const numbers = Array.from(raws).filter(raw => typeof JSON.parse(raw) === 'number').map(raw => ({raw,native:capture(JSON.parse(raw))}));
process.stdout.write(JSON.stringify({header,outcomes,refusals,callable,depth,numbers,cases:cases.length}));
