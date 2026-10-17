'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const mode = process.argv[2];
assert([undefined, 'default-nullish', 'cancel-prefix'].includes(mode), 'known source control');
const prefix = 'safe_cex_venue_grvt::';
const contracts = JSON.parse(fs.readFileSync(path.resolve(__dirname, '../../safe-cex-parity/accepted-set/contracts/safe-cex-venue-grvt.json')));
function bits(value) {
  if (Number.isNaN(value)) return 'NaN';
  const view = new DataView(new ArrayBuffer(8));
  view.setFloat64(0, value, false);
  return view.getBigUint64(0, false).toString(16).padStart(16, '0');
}
let count = 0;
for (const [owner, operation] of [
  ['data_reads::text', 'text'], ['data_reads::number', 'number'],
  ['data_reads::parsed', 'parsed'], ['data_reads::default_parse', 'default'],
  ['markets::parsed', 'default'],
]) {
  const cases = contracts[prefix + owner].cases;
  assert.equal(cases.length, 38, 'independent scalar floor');
  for (const row of cases) {
    const input = row.input;
    const value = input.absent ? undefined : Object.hasOwn(input, 'raw') ? JSON.parse(input.raw) : input.value;
    let actual;
    try {
      // These are actual ECMAScript operators, not an extracted GRVT method.
      const result = operation === 'text' ? String(value) : operation === 'number' ? Number(value)
        : operation === 'parsed' ? parseFloat(value)
        : parseFloat(mode === 'default-nullish' ? (value ?? input.fallback) : (value || input.fallback));
      actual = operation === 'text' ? { ok: true, text: result } : { ok: true, bits: bits(result) };
    } catch (error) {
      actual = { ok: false, name: error.name, message: error.message, response: error.response ?? null, code: error.code ?? null };
    }
    const { origin, ...expected } = row.output; // Native origin presence is verified only by native probes.
    assert.deepEqual(actual, expected, `SCALAR language operator ${owner} ${JSON.stringify(input)}`);
    ++count;
  }
}
{
const rows = contracts, p = prefix; let count = 0;
function optional(v){return {present:v!==undefined,value:v??null};}
for (const name of ['or','results','instrument','no_results']) {
 for(const row of rows[p+'data_reads::'+name].cases){
  const i=row.input;let actual;
  if(name==='or')actual=optional(i.a||i.b);
  else if(name==='no_results')actual=!i.v||i.v.length===0;
  else {const first=name==='results'?'results':'instrument',second=name==='results'?'r':'i';try{actual={ok:true,...optional(i[first]||i[second])};}catch(e){actual={ok:false,name:e.name,message:e.message};}}
  assert.deepEqual(actual,row.output,name+' '+JSON.stringify(i));count++;
 }
}
function typedNumbers(value){if(typeof value==='number'&&Number.isInteger(value)&&Number.isFinite(value))return BigInt(value).toString();if(Array.isArray(value))return value.map(typedNumbers);if(value&&typeof value==='object')return Object.fromEntries(Object.entries(value).map(([k,v])=>[k,typedNumbers(v)]));return value;}
for(const row of rows[p+'binding::signing::typed_numbers'].cases){const i=row.input;assert.deepEqual(typedNumbers(Object.hasOwn(i,'raw')?JSON.parse(i.raw):i.value),row.output);count++;}
for(const row of rows[p+'binding::signing::fields'].cases){assert.deepEqual(row.input.map(([name,type])=>({name,type})),row.output);count++;}
const keys={Constructor:'constructor',DefineGetter:'__defineGetter__',DefineSetter:'__defineSetter__',HasOwnProperty:'hasOwnProperty',LookupGetter:'__lookupGetter__',LookupSetter:'__lookupSetter__',IsPrototypeOf:'isPrototypeOf',PropertyIsEnumerable:'propertyIsEnumerable',ToString:'toString',ValueOf:'valueOf',Prototype:'__proto__',ToLocaleString:'toLocaleString'};
for(const row of rows[p+'public_subscriptions::interval_string'].cases){const i=row.input;assert.equal(Object.hasOwn(i,'mapped')?i.mapped:String(({})[keys[i.inherited]]),row.output);count++;}
assert.equal(count, 85, 'independent supplementary floor');
}
assert(path.isAbsolute(process.env.SAFE_CEX_TYPESCRIPT_PATH || ''), 'absolute isolated TypeScript compiler required');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const file = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/grvt/grvt.exchange.ts');
const tree = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
assert.equal(tree.parseDiagnostics.length, 0, 'maintained source syntax');
const owners = tree.statements.filter(node => ts.isClassDeclaration(node) && node.name?.text === 'GrvtExchange');
assert.equal(owners.length, 1, 'unique actual source class');
function declaredName(name) {
  assert(ts.isIdentifier(name) || ts.isStringLiteral(name) || ts.isPrivateIdentifier(name), 'static maintained member name');
  return ts.isPrivateIdentifier(name) ? name.getText(tree) : name.text;
}
function cancellationCatch(name) {
  const members = owners[0].members.filter(node => node.name && declaredName(node.name) === name);
  assert.equal(members.length, 1, `unique source member ${name}`);
  const arrow = members[0].initializer;
  assert(ts.isArrowFunction(arrow) && ts.isBlock(arrow.body), 'maintained cancellation arrow');
  const tries = arrow.body.statements.filter(ts.isTryStatement);
  assert.equal(tries.length, 1, 'unique maintained cancellation try');
  const clause = tries[0].catchClause;
  assert(clause && ts.isIdentifier(clause.variableDeclaration.name), 'named maintained catch');
  let source = `(function(${clause.variableDeclaration.name.text}) ${clause.block.getText(tree)})`;
  if (mode === 'cancel-prefix') {
    const anchor = name === 'cancelOrder' ? 'Cancel failed: ' : 'Cancel all failed: ';
    assert.equal(source.split(anchor).length, 2, 'source prefix mutation anchor');
    source = source.replace(anchor, 'drift: ');
  }
  const compiled = ts.transpileModule(source, { compilerOptions: { target: ts.ScriptTarget.ES2022 }, reportDiagnostics: true });
  assert.equal(compiled.diagnostics.length, 0, 'actual source catch transpiles');
  return vm.runInNewContext(compiled.outputText, { Error }, { filename: file });
}
const catches = { 'Cancel failed: ': cancellationCatch('cancelOrder'), 'Cancel all failed: ': cancellationCatch('cancelAllOrders') };
const cases = contracts[prefix + 'order_controls::cancellation_error'].cases;
assert.equal(cases.length, 13, 'independent cancellation floor');
let cancellations = 0;
for (const row of cases) {
  if (Object.hasOwn(row.input, 'ok')) continue; // Two typed passthrough successes are native-only evidence.
  const input = row.input;
  const cause = Object.assign(new Error('original'), { name: 'OriginalError', code: 99 });
  if (Object.hasOwn(input, 'data')) cause.response = { data: input.data };
  const emit = Object.assign(new Error('emit failed'), { name: 'EmitError', response: { emit: 1 }, code: 44, stack: 'emit stack' });
  const events = [];
  let actual;
  try {
    catches[input.prefix].call({ emitAndLogError(text) { events.push(text); if (input.emit_fail) throw emit; } }, cause);
    assert.fail('actual catch must throw');
  } catch (error) {
    actual = { ok: false, name: error.name, message: error.message, events, origin_retained: error === emit, response: error.response ?? null, code: error.code ?? null };
    // Fresh JS Error has an ambient stack. Only the explicit retained callback stack is comparable.
    if (input.emit_fail) actual.stack = error.stack;
  }
  const expected = { ...row.output };
  if (!input.emit_fail) delete expected.stack;
  assert.deepEqual(actual, expected, `CANCEL actual maintained catch ${JSON.stringify(input)}`);
  ++cancellations;
}
assert.equal(cancellations, 11, 'actual catch floor');
console.log(`${count} actual ECMAScript operator expectations, 85 selected-language/representation expectations and ${cancellations} actual GRVT catch expectations pass; native error-origin and success witnesses remain separate.`);
