'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const compiler = process.env.SAFE_CEX_TYPESCRIPT_PATH;
assert(compiler && path.isAbsolute(compiler), 'absolute isolated compiler required');
const ts = require(compiler);
const file = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/grvt/grvt.types.ts');
let source = fs.readFileSync(file, 'utf8');
// Append exact interface declarations from their owning sources. Their imports
// are unrelated to these standalone records and are not stubbed.
for (const [relative, name] of [['grvt.signing.ts', 'OrderLeg'], ['grvt.signing.ts', 'GrvtSignature'], ['grvt.signing.ts', 'OrderPayload'], ['grvt.builder.ts', 'GrvtBuilderCheckResult'], ['grvt.builder.ts', 'GrvtBuilderInfo'], ['grvt.builder.ts', 'GrvtBuilderAuthorizationResult']]) {
  const owner = path.join(path.dirname(file), relative);
  const text = fs.readFileSync(owner, 'utf8');
  const parsed = ts.createSourceFile(owner, text, ts.ScriptTarget.Latest, true);
  const declaration = parsed.statements.find(s => ts.isInterfaceDeclaration(s) && s.name.text === name);
  assert(declaration, `actual source interface ${name}`);
  source += '\n' + declaration.getText(parsed);
}


const sharedFile = path.resolve(path.dirname(file), '../../types.ts');
const sharedTree = ts.createSourceFile(sharedFile, fs.readFileSync(sharedFile, 'utf8'), ts.ScriptTarget.Latest, true);
const sharedEnum = sharedTree.statements.find(s => ts.isEnumDeclaration(s) && s.name.text === 'OrderStopTriggerType');
assert(sharedEnum, 'actual shared trigger enum');
source += '\n' + sharedEnum.getText(sharedTree);
const triggerFile = path.join(path.dirname(file), 'grvt.order-semantics.ts');
const triggerTree = ts.createSourceFile(triggerFile, fs.readFileSync(triggerFile, 'utf8'), ts.ScriptTarget.Latest, true);
const triggerAlias = triggerTree.statements.find(s => ts.isTypeAliasDeclaration(s) && s.name.text === 'GrvtTriggerMetadata');
assert(triggerAlias, 'actual trigger metadata alias');
source += '\n' + triggerAlias.getText(triggerTree);

if (process.env.GRVT_RECORD_MUTATION === 'trigger-number') {
  assert(source.includes('triggerPrice?: number;'), 'trigger number mutation matched');
  source = source.replace('triggerPrice?: number;', 'triggerPrice?: boolean;');
} else if (process.env.GRVT_RECORD_MUTATION === 'required') {
  assert(source.includes('client_order_id?: string;'), 'requiredness mutation matched');
  source = source.replace('client_order_id?: string;', 'client_order_id: string;');
} else if (process.env.GRVT_RECORD_MUTATION === 'limit-required') {
  assert(source.includes('limitPrice?: string;'), 'limit requiredness mutation matched');
  source = source.replace('limitPrice?: string;', 'limitPrice: string;');
} else if (process.env.GRVT_RECORD_MUTATION === 'lite-number') {
  assert(source.includes('bd?: number;'), 'numeric type mutation matched');
  source = source.replace('bd?: number;', 'bd?: boolean;');
} else if (process.env.GRVT_RECORD_MUTATION === 'signature-number') {
  assert(source.includes('v: number;'), 'signature numeric type mutation matched');
  source = source.replace('v: number;', 'v: boolean;');
} else assert(!process.env.GRVT_RECORD_MUTATION, 'unknown record mutation');
const options = { strict: true, types: [], skipLibCheck: true, target: ts.ScriptTarget.ES2022 };
function program(text) {
  const host = ts.createCompilerHost(options);
  const read = host.readFile;
  host.readFile = name => name === file ? text : read(name);
  return ts.createProgram([file], options, host);
}
const base = program(source);
assert.equal(base.getSemanticDiagnostics().length, 0, 'baseline TypeScript record diagnostics');
// CHOSEN wire-data boundary: source `any` also permits executable values.
// Record typeof before JSON.stringify erases these fields; these are not JSON
// inputs that the Rust decoder falsely calls malformed.
const executable = program(source + '\nconst executableProbe: GrvtWebSocketMessage = {jsonrpc: "2.0", params: () => 1, result: Symbol("r"), error: {code: 1, message: "", data: () => 2}};');
assert.equal(executable.getSemanticDiagnostics().length, 0, 'TS any admits executable payload');
const executableValue = {jsonrpc: '2.0', params: () => 1, result: Symbol('r'), error: {code: 1, message: '', data: () => 2}};
assert.deepEqual([typeof executableValue.params, typeof executableValue.result, typeof executableValue.error.data], ['function', 'symbol', 'function'], 'pre-serialization executable any witness');
assert.equal(JSON.stringify(executableValue), '{"jsonrpc":"2.0","error":{"code":1,"message":""}}', 'executable payload omitted at JSON wire boundary');
const tree = base.getSourceFile(file), checker = base.getTypeChecker();
const names = ['GrvtInstrument', 'GrvtInstrumentLite', 'GrvtTicker', 'GrvtOrderBookLevel', 'GrvtOrderBook', 'GrvtTrade', 'GrvtKline', 'GrvtPosition', 'GrvtBalance', 'GrvtAccountSummary', 'GrvtOrder', 'GrvtAuthRequest', 'GrvtAuthResponse', 'GrvtFill', 'OrderLeg', 'GrvtBuilderAuthorizationResult', 'GrvtSignature', 'OrderPayload', 'GrvtBuilderCheckResult', 'GrvtBuilderInfo', 'GrvtWebSocketSubscription', 'GrvtWebSocketMessage', 'GrvtTriggerMetadata'];
function sample(type) {
  if (type.flags & ts.TypeFlags.Any) return { n: 1.25 };
  if (type.isUnion()) return sample(type.types.find(t => !(t.flags & ts.TypeFlags.Undefined)));
  if (type.flags & ts.TypeFlags.StringLiteral) return type.value;
  if (type.flags & ts.TypeFlags.String) return 'scalar😀\ufeff';
  if (type.flags & ts.TypeFlags.BooleanLike) return true;
  if (type.flags & ts.TypeFlags.NumberLike) return 1.25;
  if (checker.isArrayType(type)) return [sample(checker.getTypeArguments(type)[0])];
  const fields = checker.getPropertiesOfType(type);
  assert(fields.length, `unhandled field type ${checker.typeToString(type)}`);
  return Object.fromEntries(fields.map(field => [field.name, sample(checker.getTypeOfSymbolAtLocation(field, field.valueDeclaration))]));
}
const rows = [];
const identityAudit = [];
function add(name, label, value, boundary = false) {
  rows.push({ name, label, raw: JSON.stringify(value), boundary });
}
for (const name of names) {
  const declaration = tree.statements.find(s => s.name?.getText(tree) === name);
  assert(declaration, `missing record ${name}`);
  const value = sample(checker.getTypeAtLocation(declaration));
  add(name, 'all-fields', value);
  const leaves = [];
  function collect(value, parts = []) {
    if (value !== null && typeof value === 'object') {
      for (const [key, child] of Object.entries(value)) collect(child, [...parts, key]);
    } else leaves.push({path:parts.join('.'),kind:typeof value,value});
  }
  collect(value);
  const equalGroups = [];
  for (const leaf of leaves) {
    const existing = equalGroups.find(group => group.kind === leaf.kind && Object.is(group.value, leaf.value));
    if (existing) existing.paths.push(leaf.path);
    else equalGroups.push({kind:leaf.kind,value:leaf.value,paths:[leaf.path]});
  }
  identityAudit.push({name, leaves:leaves.length, equalGroups:equalGroups.filter(group => group.paths.length > 1)});
  for (const shape of [null, true, 1, 'record', [], Object.values(value)]) add(name, 'root-shape', shape);
  for (const field of Object.keys(value)) {
    const missing = { ...value }; delete missing[field];
    add(name, `missing:${field}`, missing);
    add(name, `null:${field}`, { ...value, [field]: null });
    add(name, `wrong-kind:${field}`, { ...value, [field]: typeof value[field] === 'boolean' ? 'true' : false });
    if (typeof value[field] === 'number') {
      for (const wrong of ['', '1', [], {}]) add(name, `number-shape:${field}`, { ...value, [field]: wrong });
    }
    if (Array.isArray(value[field]) && typeof value[field][0] === 'string') {
      add(name, `array-string:${field}`, { ...value, [field]: ['😀\ufeff'] });
      add(name, `array-surrogate:${field}`, { ...value, [field]: ['\ud800'] }, true);
    }
    if (typeof value[field] === 'string') {
      // Both ordinary strings and union fields are measured by the compiler below.
      for (const text of ['', ' \t\ufeff', '汉字😀', '\ud800', '\udfff']) {
        add(name, `string:${field}:${JSON.stringify(text)}`, { ...value, [field]: text }, /[\ud800-\udfff]/u.test(text));
      }
    }
  }
  function replaceAt(parts, replacement, remove = false, boundary = false) {
    const copy = JSON.parse(JSON.stringify(value));
    let target = copy;
    for (const part of parts.slice(0, -1)) target = target[part];
    if (remove) delete target[parts.at(-1)];
    else target[parts.at(-1)] = replacement;
    add(name, `nested-or-union:${parts.join('.')}`, copy, boundary);
  }
  function refine(type, parts = []) {
    if (type.flags & ts.TypeFlags.Any) {
      for (const value of [null, true, false, 1.25, "", "😀", [], [1.25], {n: 1.25}]) replaceAt(parts, value);
      replaceAt(parts, "\ud800", false, true);
      return;
    }
    if (type.isUnion()) {
      const members = type.types.filter(t => !(t.flags & ts.TypeFlags.Undefined));
      if (members.every(t => t.flags & ts.TypeFlags.StringLiteral)) {
        for (const member of members) replaceAt(parts, member.value);
        return;
      }
      for (const member of members) refine(member, parts);
      return;
    }
    if (checker.isArrayType(type)) {
      const element = checker.getTypeArguments(type)[0];
      for (const array of [[], [null], [false], [Object.values(sample(element))]]) replaceAt(parts, array);
      refine(element, [...parts, 0]);
      return;
    }
    if (type.flags & ts.TypeFlags.String) {
      replaceAt(parts, `identity:${parts.join('.')}:😀`);
      return;
    }
    if (type.flags & ts.TypeFlags.BooleanLike) {
      replaceAt(parts, false);
      return;
    }
    if (type.flags & ts.TypeFlags.NumberLike) {
      replaceAt(parts, 17.25);
      return;
    }
    if (type.flags & ts.TypeFlags.StringLike) return;
    for (const field of checker.getPropertiesOfType(type)) {
      const next = [...parts, field.name];
      if (parts.length) {
        replaceAt(next, null);
        replaceAt(next, false);
        replaceAt(next, undefined, true);
      }
      refine(checker.getTypeOfSymbolAtLocation(field, field.valueDeclaration), next);
    }
  }
  refine(checker.getTypeAtLocation(declaration));
}
// String enums require enum-member expressions in the checker, while their
// JSON wire values are strings. Resolve that bridge from the ACTUAL enum AST.
const triggerMembers = new Map(sharedEnum.members.map(m => {
  assert(ts.isStringLiteral(m.initializer), 'shared trigger string literal');
  return [m.initializer.text, m.name.getText(sharedTree)];
}));
function probeExpression(row) {
  if (row.name !== 'GrvtTriggerMetadata') return row.raw;
  const value = JSON.parse(row.raw);
  const member = value && !Array.isArray(value) && triggerMembers.get(value.triggerType);
  return member ? row.raw.replace('"triggerType":' + JSON.stringify(value.triggerType), '"triggerType":OrderStopTriggerType.' + member) : row.raw;
}
const augmented = source + '\n' + rows.map((row, i) => `const grvtProbe${i}: ${row.name} = ${probeExpression(row)};`).join('\n');
const checked = program(augmented);
const checkedTree = checked.getSourceFile(file);
const diagnostics = checked.getSemanticDiagnostics();
const statements = checkedTree.statements.filter(s => ts.isVariableStatement(s) && s.declarationList.declarations[0].name.getText(checkedTree).startsWith('grvtProbe'));
assert.equal(statements.length, rows.length);
rows.forEach((row, i) => {
  row.accepted = !diagnostics.some(d => d.file?.fileName === file && d.start >= statements[i].getStart() && d.start < statements[i].end);
  row.boundary = row.boundary && row.accepted;
});
if (process.argv.includes('--identity-audit')) {
  process.stdout.write(JSON.stringify({records:identityAudit, identityCases:rows.filter(row => row.label.startsWith('nested-or-union:') && row.accepted).length, totalCases:rows.length, boundaryCases:rows.filter(row => row.boundary).length}, null, 2));
} else process.stdout.write(JSON.stringify(rows));
