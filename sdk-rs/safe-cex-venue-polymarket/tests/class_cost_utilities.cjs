// Finite, reproducible differential profile; executes the maintained source bodies.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const crypto = require('node:crypto');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const file = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/polymarket/polymarket.exchange.ts');
const source = fs.readFileSync(file, 'utf8');
const tree = ts.createSourceFile(file, source, ts.ScriptTarget.Latest, true);
const names = ['normalizeTickSize', 'parseGammaMarketTokens', 'precisionFromTickSize'];
const selected = tree.statements.filter(node =>
  (ts.isFunctionDeclaration(node) && names.includes(node.name?.text)) ||
  (ts.isVariableStatement(node) && node.declarationList.declarations.some(d => d.name.getText(tree) === 'SUPPORTED_TICK_SIZES')));
assert.equal(selected.length, 4, 'generated_actual_declarations');
const original = selected.map(node => source.slice(node.getFullStart(), node.end)).join('\n');
const compiled = ts.transpileModule(original + names.map(name => `\nexports.${name} = ${name};`).join(''), {
  compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
}).outputText;
const context = { exports: {} };
vm.runInNewContext(compiled, context, { filename: 'maintained-polymarket-utilities.ts' });
const bits = value => { const b = Buffer.alloc(8); b.writeDoubleBE(value); return b.toString('hex'); };
const number = hex => { const b = Buffer.from(hex, 'hex'); return b.readDoubleBE(); };
const units = value => Array.from({length:value.length}, (_,i) => value.charCodeAt(i));
const encode = value => value === null ? {kind:'null'} : typeof value === 'number' ? {kind:'number',bits:bits(value)} : {kind:'string',value};
let state = 0x0123456789abcdefn;
const next = () => (state = BigInt.asUintN(64, state * 6364136223846793005n + 1442695040888963407n));
const tickStrings = ['0.1','0.01','0.005','0.0025','0.001','0.0001'];
// Preserve every old positive length 0..32, then probe larger cardinality
// boundaries. This is a finite profile, not coverage of all array lengths.
const largerTokenLengths = [33,34,63,64,65,66,127,128,129,130,255,256,257,258,
  511,512,513,514,1023,1024,1025,47,48,49,95,96,97,191,192,193,194];
assert.equal(largerTokenLengths.length,31,'generated_token_boundary_profile_slots');
const family=process.argv[2]; assert(['ticks','tokens','precisions'].includes(family),'selected utility family');
const ticks = [], tokens = [], precisions = [];
for (let i = 0; i < 512; i++) {
  const group = i % 8, n = Math.floor(i / 8), tick = tickStrings[n % 6];
  const random = number((next() & 0xffefffffffffffffn).toString(16).padStart(16,'0'));
  const value = [tick, Number(tick), ` ${tick} `, null, random, `😀é-${n}`, '', -n][group];
  const item = {index:i,input:encode(value)};
  if(family==='ticks'){
  try { item.ok = context.exports.normalizeTickSize(value); }
  catch (error) { item.errorName = error.name; item.error = units(error.message); }
  ticks.push(item); }

  const length = n < 33 ? n : largerTokenLengths[n - 33];
  const ids = Array.from({length}, (_,j) => `token-${n}-${j}`);
  const outcomes = Array.from({length}, (_,j) => `outcome-${n}-${j}`);
  let pair;
  switch (group) {
    case 0: pair = [JSON.stringify(ids), JSON.stringify(outcomes)]; break;
    case 1: pair = [JSON.stringify(ids), JSON.stringify([...outcomes, 'extra'])]; break;
    case 2: pair = ['[broken', JSON.stringify(outcomes)]; break;
    case 3: pair = [JSON.stringify({n}), JSON.stringify(outcomes)]; break;
    case 4: pair = [JSON.stringify([null, n, `id-${n}`, {}, []]), JSON.stringify(['skip','skip',`yes-${n}`,'skip','skip'])]; break;
    case 5: pair = [n % 2 ? null : '', JSON.stringify(outcomes)]; break;
    case 6: pair = [JSON.stringify([`😀-${n}`,`é-${n}`]), JSON.stringify([`Yes-${n}`,`No-${n}`])]; break;
    case 7: pair = [JSON.stringify([`same-${n}`,`same-${n}`]), JSON.stringify(['Yes','No'])]; break;
  }
  if(family==='tokens'){
  const output = context.exports.parseGammaMarketTokens({clobTokenIds:pair[0],outcomes:pair[1]});
  tokens.push({index:i,tokenIds:pair[0],outcomes:pair[1],output:output.map(p => ({tokenId:units(p.tokenId),outcome:units(p.outcome)}))}); }

  const halfway = (2*n+1)/512;
  const adjacent = BigInt('0x'+bits(halfway)) + BigInt(n%3-1);
  const precisionInput = [random, number(adjacent.toString(16).padStart(16,'0')), tick,
    ['0x10','0b11','1_000','NaN','Infinity',' 0.0025 ','\ufeff0.001','1.23x'][n%8],
    -n, [NaN,Infinity,-Infinity,-0][n%4], [1e21,1e22,1e30,1e100,1e300][n%5], `${n}.${String(n*73).padStart(8,'0')}`][group];
  if(family==='precisions')precisions.push({index:i,input:encode(precisionInput),outputBits:bits(context.exports.precisionFromTickSize(precisionInput))});
}
process.stdout.write(JSON.stringify({capture:{node:process.version,architecture:process.arch,typescript:ts.version,
  sourceSha256:crypto.createHash('sha256').update(source).digest('hex'),seed:'0123456789abcdef',casesPerRow:512},ticks,tokens,precisions}));
