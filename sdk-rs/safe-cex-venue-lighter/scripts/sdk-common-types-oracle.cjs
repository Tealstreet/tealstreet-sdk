const fs = require('node:fs');
const path = require('node:path');
const assert = require('node:assert/strict');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const file = path.resolve(__dirname, '../../../packages/lighter-sdk/src/types/common.ts');
let source = fs.readFileSync(file, 'utf8');
const mutation = process.env.LIGHTER_SDK_COMMON_MUTATION;
if (mutation === 'inherited-number') source = source.replace(/(interface Balance(?:<[^>]+>)?)(\s*\{)/, '$1 extends ExtraNumeric$2') + '\ninterface ExtraNumeric { hiddenNumeric: number; }\n';
if (mutation === 'field') source = source.replace('decimals: number;', 'decimals: string;');
if (mutation === 'optional') source = source.replace('page?: number;', 'page: number;');
if (mutation === 'enum') source = source.replace('Active = "active"', 'Active = "changed"');
if (mutation === 'config-optional') source = source.replace('debug?: boolean;', 'debug: boolean;');
if (mutation === 'account-array') source = source.replace('balances: Balance[];', 'balances: OrderBookLevel[];');
if (mutation === 'market-status') source = source.replace('status: MarketStatus;', 'status: string;');
if (mutation === 'position-side') source = source.replace('side: "long" | "short";', 'side: "buy" | "sell";');
if (mutation === 'trade-maker') source = source.replace('isBuyerMaker: boolean;', 'isBuyerMaker: number;');
if (mutation === 'book-levels') source = source.replace('bids: OrderBookLevel[];', 'bids: Balance[];');
const tree = ts.createSourceFile(file, source, ts.ScriptTarget.Latest, true);
const selected = ['Balance', 'Ticker', 'OrderBookLevel', 'PaginationParams', 'LighterConfig', 'Account', 'Market', 'Position', 'Trade', 'OrderBook'];
const schemas = Object.fromEntries(selected.map(name => {
  const node = tree.statements.find(n => ts.isInterfaceDeclaration(n) && n.name.text === name);
  assert.ok(node, name);
  assert.ok(!node.heritageClauses?.length, `${name} unsupported interface heritage`);
  return [name, node.members.map(member => {
    const type = member.type.getText(tree);
    assert.ok(['string', 'number', 'boolean', 'Balance[]', 'OrderBookLevel[]', 'MarketStatus', '"long" | "short"', '"buy" | "sell"'].includes(type), `${name} primitive type`);
    return { name: member.name.text, type, optional: !!member.questionToken };
  })];
}));
const status = tree.statements.find(n => ts.isEnumDeclaration(n) && n.name.text === 'MarketStatus');
const statuses = status.members.map(member => {
  assert.ok(ts.isStringLiteral(member.initializer), 'MarketStatus string member');
  return [member.name.text, member.initializer.text];
});
function sample(type, text = 'fixture') {
  if (type === 'string') return text;
  if (type === 'number') return 16777217.25;
  if (type === 'boolean') return true;
  if (type === 'MarketStatus') return 'active';
  if (type.includes(' | ')) return JSON.parse(type.split(' | ')[0]);
  if (type.endsWith('[]')) return [Object.fromEntries(schemas[type.slice(0,-2)].map(f => [f.name, sample(f.type, text)]))];
  throw new Error(`unsupported sample ${type}`);
}
const witnesses = {};
for (const [name, fields] of Object.entries(schemas)) {
  witnesses[name] = [];
  const strings = fields.filter(field => field.type === 'string');
  for (const text of ['plain', 'é', '👩', '\ufeff', '\u0085', '\ud800', '\udfff', 'x\ud800y']) {
    if (!strings.length) continue;
    const value = Object.fromEntries(fields.map(field => [field.name, sample(field.type, text)]));
    const encoded = JSON.stringify(value);
    const parsed = JSON.parse(encoded);
    const units = Array.from({length:text.length}, (_,i) => text.charCodeAt(i));
    assert.ok(strings.every(field => parsed[field.name] === text), `${name} source JSON string retention`);
    witnesses[name].push({ encoded, units, representable: text.isWellFormed() });
  }
}
const overflow = {};
for (const [name, fields] of Object.entries(schemas)) {
  overflow[name] = [];
  for (const field of fields.filter(f => f.type === 'number')) for (const lexeme of ['1e400', '-1e400', '-0', '-0.0', '0']) {
    const raw = '{' + fields.map(f => JSON.stringify(f.name) + ':' + (f.name === field.name ? lexeme : JSON.stringify(sample(f.type)))).join(',') + '}';
    const value = JSON.parse(raw)[field.name];
    const bytes = Buffer.alloc(8); bytes.writeDoubleBE(value);
    overflow[name].push({ raw, field: field.name, bits: bytes.toString('hex') });
  }
}
process.stdout.write(JSON.stringify({ schemas, statuses, witnesses, overflow }));
