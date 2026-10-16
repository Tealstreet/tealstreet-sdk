const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const {createRequire} = require('node:module');
assert(process.env.SAFE_CEX_TYPESCRIPT_PATH, 'APEX-SURROGATE compiler required');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const dependency = createRequire(path.join(process.env.SAFE_CEX_TYPESCRIPT_PATH, 'package.json'));
const root = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/apexomni');
function declaration(file, name) {
 const text = fs.readFileSync(path.join(root,file),'utf8');
 const tree = ts.createSourceFile(file,text,ts.ScriptTarget.Latest,true);
 const found = tree.statements.filter(node => ts.isFunctionDeclaration(node) ? node.name?.text === name : ts.isVariableStatement(node) && node.declarationList.declarations.some(d => d.name.getText(tree) === name));
 assert.equal(found.length,1,'APEX-SURROGATE actual declaration '+name);
 return found[0].getText(tree);
}
const names = ['serializeApexPayload','serializeApexPayloadLegacy','generateApeXRandomClientId'];
const control = process.env.SAFE_CEX_APEX_SURROGATE_CONTROL;
assert(!control || names.includes(control),'APEX-SURROGATE known control');
const selected = names.map(name => {
 let text = declaration(name === names[2] ? 'apexomni.exchange.ts' : 'apexomni.api.ts',name);
 if (name === control) {
  const before = name === names[2] ? '${accountId}' : 'return value;';
  const after = name === names[2] ? '${accountId.replaceAll(String.fromCharCode(0xd800),String.fromCharCode(0xd801))}' : "return typeof value === 'string' ? value.replaceAll(String.fromCharCode(0xd800),String.fromCharCode(0xd801)) : value;";
  assert.equal(text.split(before).length,2,'APEX-SURROGATE exact mutation selector');
  text = text.replace(before,after);
 }
 return text;
});
selected.push(declaration('apexomni.exchange.ts','d8'));
const math = Object.create(Math);math.random = () => 0.25;
const api = new vm.Script(ts.transpileModule(selected.join('\n'),{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText+'\n({'+names.join(',')+'})').runInNewContext({exports:{},qs:dependency('qs'),Date:{now:()=>1234},Math:math});
const cases = [
 ['high',String.raw`{"x":"\ud800"}`],
 ['low',String.raw`{"x":"\udc00"}`],
 ['high-tail',String.raw`{"x":"\ud800A"}`],
 ['low-prefix',String.raw`{"x":"A\udc00"}`],
 ['pair',String.raw`{"x":"\ud83d\ude00"}`],
 ['scalar',String.raw`{"x":"é"}`],
 ['replacement',String.raw`{"x":"\ufffd"}`],
 ['ascii-escape',String.raw`{"x":"\\ud800"}`],
 ['whitespace',String.raw`{"x":"\ufeff\u0085"}`],
 ['empty',String.raw`{"x":""}`],
];
const units = value => Array.from({length:value.length},(_,i)=>value.charCodeAt(i));
const rows = cases.map(([label,json]) => {
 const payload = JSON.parse(json);
 const results = Object.fromEntries(names.map(name => {
  const value = name === names[2] ? api[name](payload.x) : api[name](payload);
  return [name,{type:typeof value,units:units(value)}];
 }));
 return {label,json,inputType:typeof payload.x,inputUnits:units(payload.x),results};
});
assert.equal(rows.length,10,'APEX-SURROGATE independent source floor');
console.log(JSON.stringify({header:{node:process.version,architecture:process.arch},rows}));
