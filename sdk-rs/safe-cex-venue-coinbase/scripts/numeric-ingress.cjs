const fs = require('node:fs');
const path = require('node:path');
const assert = require('node:assert/strict');
const {execFileSync} = require('node:child_process');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH ?? 'typescript');
const {captureHeader, numberBits} = require('./special-numbers.cjs');
const sourcePath = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/coinbase/coinbase.types.ts');
const source = ts.createSourceFile(sourcePath, fs.readFileSync(sourcePath, 'utf8'), ts.ScriptTarget.Latest, true);
const aliases = new Map(source.statements.filter(ts.isTypeAliasDeclaration).map(node => [node.name.text, node.type]));
const roots = ['MARKET_RESPONSE', 'INTX_INSTRUMENTS_RESPONSE', 'PORTFOLIO_BREAKDOWN_RESPONSE'];
const expected = [
  'MARKET_RESPONSE.num_products',
  'INTX_INSTRUMENTS_RESPONSE.0.price_band_percent',
  'INTX_INSTRUMENTS_RESPONSE.0.market_order_percent',
  'INTX_INSTRUMENTS_RESPONSE.0.position_limit_adq_pct',
  'INTX_INSTRUMENTS_RESPONSE.0.base_imf',
  'PORTFOLIO_BREAKDOWN_RESPONSE.breakdown.spot_positions.0.total_balance_fiat',
  'PORTFOLIO_BREAKDOWN_RESPONSE.breakdown.spot_positions.0.total_balance_crypto',
  'PORTFOLIO_BREAKDOWN_RESPONSE.breakdown.spot_positions.0.available_to_trade_fiat',
  'PORTFOLIO_BREAKDOWN_RESPONSE.breakdown.spot_positions.0.allocation',
  'PORTFOLIO_BREAKDOWN_RESPONSE.breakdown.spot_positions.0.available_to_trade_crypto',
  'PORTFOLIO_BREAKDOWN_RESPONSE.breakdown.spot_positions.0.unrealized_pnl',
  'PORTFOLIO_BREAKDOWN_RESPONSE.breakdown.spot_positions.0.available_to_transfer_fiat',
  'PORTFOLIO_BREAKDOWN_RESPONSE.breakdown.spot_positions.0.available_to_transfer_crypto',
];
const numeric = [];
function visit(node, root, parts) {
  assert.ok(node, 'live numeric source declaration exists');
  if (node.kind === ts.SyntaxKind.NumberKeyword) numeric.push({root, path: parts});
  else if (ts.isTypeReferenceNode(node)) {
    if (node.typeName.getText(source) === 'Array') visit(node.typeArguments[0], root, [...parts, 0]);
    else visit(aliases.get(node.typeName.getText(source)), root, parts);
  } else if (ts.isArrayTypeNode(node)) visit(node.elementType, root, [...parts, 0]);
  else if (ts.isTypeLiteralNode(node)) {
    for (const member of node.members) {
      assert.ok(ts.isPropertySignature(member) && member.type, 'live record member is typed');
      visit(member.type, root, [...parts, member.name.text]);
    }
  } else if (ts.isUnionTypeNode(node)) {
    for (const variant of node.types) visit(variant, root, parts);
  } else assert.ok([ts.SyntaxKind.StringKeyword, ts.SyntaxKind.BooleanKeyword].includes(node.kind) || ts.isLiteralTypeNode(node), `unsupported numeric inventory shape: ${node.getText(source)}`);
}
for (const root of roots) visit(aliases.get(root), root, []);
assert.equal(numeric.length, 13, 'independent live numeric field floor');
assert.deepEqual(numeric.map(item => [item.root, ...item.path].join('.')).sort(), [...expected].sort(), 'live numeric field content pin');
const records = JSON.parse(execFileSync(process.execPath, [path.join(__dirname, 'unions.cjs')], {encoding: 'utf8'})).legacyRecords;
const templates = {
  MARKET_RESPONSE: records.MarketResponse,
  INTX_INSTRUMENTS_RESPONSE: [records.IntxInstrumentsResponseItem],
  PORTFOLIO_BREAKDOWN_RESPONSE: records.PortfolioBreakdownResponse,
};
const spellings = ['-0', '-0.0', '-0e0', '-1e-400', '0', '0.0', '1', '-1', '1.25', '-3.5', '1e3', '1e-3', '9007199254740993', '-9007199254740993', '1e400', '-1e400', '5e-324', '-5e-324', '1.7976931348623157e308', '-1.7976931348623157e308'];
function at(value, parts) { for (const key of parts) value = value[key]; return value; }
const cases = [];
for (const item of numeric) for (const spelling of spellings) {
  const value = structuredClone(templates[item.root]);
  at(value, item.path.slice(0, -1))[item.path.at(-1)] = '__coinbase_numeric_token__';
  const body = JSON.stringify(value);
  assert.equal(body.split('"__coinbase_numeric_token__"').length, 2, 'one numeric substitution');
  const raw = body.replace('"__coinbase_numeric_token__"', spelling);
  const native = at(JSON.parse(raw), item.path);
  assert.equal(typeof native, 'number', 'source native numeric type');
  cases.push({...item, spelling, raw, bits: numberBits(native)});
}
// Existing focused owner witnesses consume these native captures as well.
const fields = numeric.map(item => {
  const type = item.root === 'MARKET_RESPONSE' ? 'MarketResponse' : item.root === 'INTX_INSTRUMENTS_RESPONSE' ? 'IntxInstrumentsResponseItem' : 'PortfolioBreakdownResponseBreakdownSpotPositionsItem';
  const rootCases = cases.filter(value => value.root === item.root && value.path.join('.') === item.path.join('.'));
  const zero = rootCases.find(value => value.spelling === '-0');
  const overflow = rootCases.find(value => value.spelling === '1e400');
  const zeroNative = at(JSON.parse(zero.raw), item.path);
  const overflowNative = at(JSON.parse(overflow.raw), item.path);
  return {type, name: item.path.at(-1), negativeZero: Object.is(zeroNative, -0), bits: zero.bits, overflowAccepted: typeof overflowNative === 'number', overflowInfinite: overflowNative === Infinity};
});
assert.equal(cases.length, 260, 'independent source spelling floor');
process.stdout.write(JSON.stringify({...captureHeader(), source: 'JSON.parse', fields, numeric, spellings, cases}));
