const fs = require('node:fs');
const path = require('node:path');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const root = path.resolve(__dirname, '../../..');
const file = path.join(root, 'packages/safe-cex/src/exchanges/hyperliquid/hyperliquid.types.ts');
const ast = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
require('./interface-heritage.cjs')(ts,ast);
const unions = {};
const pascal = s => s.replace(/(^|[_:-])(\w)/g, (_, a, b) => b.toUpperCase());
function visit(node, name) {
  if (ts.isParenthesizedTypeNode(node)) return visit(node.type, name);
  if (ts.isLiteralTypeNode(node) && ts.isStringLiteral(node.literal)) { unions[name] = [node.literal.text]; return; }
  if (ts.isUnionTypeNode(node)) {
    const values = node.types.filter(n => n.getText(ast) !== 'null');
    if (values.length < node.types.length) { if (values.length === 1) return visit(values[0], name); return visit(ts.factory.createUnionTypeNode(values), name); }
    if (values.every(n => ts.isLiteralTypeNode(n) && ts.isStringLiteral(n.literal))) { unions[name] = values.map(n => n.literal.text); return; }
    return values.forEach((n, i) => visit(n, name + 'Case' + i));
  }
  if (ts.isArrayTypeNode(node)) return visit(node.elementType, name + 'Item');
  if (ts.isTupleTypeNode(node)) return node.elements.forEach((n, i) => visit(n, name + 'Item' + i));
  if (ts.isTypeLiteralNode(node)) return node.members.filter(ts.isPropertySignature).forEach(n => visit(n.type, name + pascal(n.name.text)));
}
const notModeled = require('./not-modeled-types.cjs');
for (const node of ast.statements) {
  if ((ts.isTypeAliasDeclaration(node) || ts.isInterfaceDeclaration(node)) && notModeled.has(node.name.text)) continue;
  if (ts.isTypeAliasDeclaration(node)) visit(node.type, node.name.text);
  if (ts.isInterfaceDeclaration(node)) node.members.filter(ts.isPropertySignature).forEach(n => visit(n.type, node.name.text + pascal(n.name.text)));
}
const corpus = Object.entries(unions).map(([name, accepted]) => ({name, accepted,
 rejected: [...new Set(['', '__unknown__', ...accepted.flatMap(value => [value.toUpperCase(), value.toLowerCase(), ` ${value}`, `${value} `, `\ufeff${value}`, `${value}\u0085`])])].filter(value => !accepted.includes(value)).map(JSON.stringify)
  .concat(['null', 'false', '123', '[]', '{}', ...accepted.map(value => JSON.stringify({[value]:null})), ...accepted.map(value => JSON.stringify([value]))])
}));
const output = JSON.stringify(corpus, null, 2) + '\n';
const fixture = path.join(__dirname, '../tests/fixtures/literal-unions.json');
if (process.argv.includes('--capture')) fs.writeFileSync(fixture, output, {flag:'wx'});
else if (fs.readFileSync(fixture, 'utf8') !== output) throw Error('live TS literal accepted-set drift');
