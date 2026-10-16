// Captures the pending mapOrder boundary before shared TableHit integration.
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const root = path.resolve(__dirname, '../../..');
function ast(file) {
  return ts.createSourceFile(file, fs.readFileSync(path.join(root, file), 'utf8'), ts.ScriptTarget.Latest, true);
}
function declarations(tree, names) {
  const found = new Map();
  function visit(node) {
    if (ts.isVariableDeclaration(node) && names.includes(node.name.getText(tree))) {
      assert.ok(!found.has(node.name.getText(tree)), 'unique source declaration');
      found.set(node.name.getText(tree), node.getText(tree));
    }
    ts.forEachChild(node, visit);
  }
  visit(tree);
  return names.map(name => { assert.ok(found.has(name), name); return 'const ' + found.get(name) + ';'; }).join('\n');
}
const domain = ast('packages/safe-cex/src/types.ts');
const enums = ['OrderType', 'OrderTimeInForce'];
const enumSource = domain.statements.filter(node => ts.isEnumDeclaration(node) && enums.includes(node.name.text)).map(node => node.getText(domain)).join('\n');
assert.equal(domain.statements.filter(node => ts.isEnumDeclaration(node) && enums.includes(node.name.text)).length, 2);
const tables = declarations(ast('packages/safe-cex/src/exchanges/lighter/lighter.types.ts'), ['ORDER_TYPE', 'ORDER_TIME_IN_FORCE']);
const exchange = ast('packages/safe-cex/src/exchanges/lighter/lighter.exchange.ts');
let locals = declarations(exchange, ['typeByNumericCode', 'tifByNumericCode', 'normalizedType', 'normalizedTif']);
// Locate the actual result-OR expressions, not copied equivalents.
const expressions = {};
function visit(node) {
  if (ts.isVariableDeclaration(node) && node.initializer) {
    const text = node.initializer.getText(exchange);
    if (text.startsWith('ORDER_TYPE[normalizedType]')) expressions.type = text;
    if (text.includes('tifByNumericCode[String(rawTif')) expressions.timeInForce = text;
  }
  ts.forEachChild(node, visit);
}
visit(exchange);
assert.deepEqual(Object.keys(expressions).sort(), ['timeInForce', 'type']);
if (process.env.LIGHTER_TABLE_MUTATION === 'own-only') {
  expressions.type = expressions.type.replace("typeByNumericCode[String(rawType ?? '')]", "(Object.hasOwn(typeByNumericCode, String(rawType ?? '')) ? typeByNumericCode[String(rawType ?? '')] : undefined)");
}
if (process.env.LIGHTER_TABLE_MUTATION === 'wrong-own-function') locals += '\ntypeByNumericCode.toString = Object.prototype.constructor as any;';
const cases = [];
for (const raw of ['constructor','toString','valueOf','hasOwnProperty','isPrototypeOf','propertyIsEnumerable','toLocaleString','__proto__','__defineGetter__','__defineSetter__','__lookupGetter__','__lookupSetter__','unknown','0','1','LIMIT','GTC']) {
  const code = enumSource + '\n' + tables + '\n' + locals + '\n' +
    `exports.result = {type: (${expressions.type}), timeInForce: (${expressions.timeInForce})};
     exports.metadata = {
       type: {own:Object.hasOwn(typeByNumericCode,rawType), matchesPrototype:exports.result.type===(rawType==='__proto__'?Object.prototype:Object.prototype[rawType])},
       timeInForce: {own:Object.hasOwn(tifByNumericCode,rawTif), matchesPrototype:exports.result.timeInForce===(rawTif==='__proto__'?Object.prototype:Object.prototype[rawTif])}
     };`;
  const context = {exports: {}, rawType: raw, rawTif: raw, postOnly: false};
  vm.runInNewContext(ts.transpileModule(code, {compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText, context);
  const result = Object.fromEntries(Object.entries(context.exports.result).map(([key,value]) => [key, {
    kind: context.exports.metadata[key].matchesPrototype && !context.exports.metadata[key].own ? 'Inherited' : typeof value === 'string' ? 'Mapped' : 'Other',
    own: context.exports.metadata[key].own,
    matchesPrototype: context.exports.metadata[key].matchesPrototype,
    type: typeof value,
    truthy: !!value,
    ...(typeof value === 'string' ? {value} : {member:context.exports.metadata[key].matchesPrototype?raw:null}),
  }]));
  cases.push({raw,result});
}
if(process.env.LIGHTER_TABLE_MUTATION==='duplicate-inherited')cases[1]=cases[0];
if(process.env.LIGHTER_TABLE_MUTATION==='empty-cases')cases.length=0;
if(process.env.LIGHTER_TABLE_MUTATION==='short-cases')cases.pop();
process.stdout.write(JSON.stringify(cases));
