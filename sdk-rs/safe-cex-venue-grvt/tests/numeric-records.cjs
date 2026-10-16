'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const compiler = process.env.SAFE_CEX_TYPESCRIPT_PATH;
assert(compiler && path.isAbsolute(compiler), 'absolute isolated compiler required');
const ts = require(compiler);
const directory = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/grvt');
function readTree(file) {
  let text = fs.readFileSync(file, 'utf8');
  const target = process.env.GRVT_NUMERIC_HERITAGE_MUTATION;
  if (target) {
    assert(['GrvtInstrumentLite', 'GrvtWebSocketMessage'].includes(target), 'known heritage control');
    const needle = `interface ${target} {`;
    if (text.includes(needle)) text = 'interface GrvtHiddenNumeric { hiddenNumeric: number; }\n' + text.replace(needle, `interface ${target} extends GrvtHiddenNumeric {`);
  }
  const tree = ts.createSourceFile(file, text, ts.ScriptTarget.Latest, true);
  for (const declaration of tree.statements.filter(ts.isInterfaceDeclaration)) {
    assert(!declaration.heritageClauses?.length, `GRVT numeric census refuses inherited fields: ${declaration.name.text}`);
  }
  return tree;
}
const models = [
  ['grvt.order-semantics.ts', 'GrvtTriggerMetadata', ['triggerPrice'], {type:'STOP_LOSS'}],
  ['grvt.types.ts', 'GrvtWebSocketSubscription', ['id'], {jsonrpc:'2.0',method:'subscribe',params:{channel:'v1.ticker'}}],
  ['grvt.types.ts', 'GrvtInstrumentLite', ['bd', 'qd', 'fi'], {i:'BTC_USDT_Perp',ih:'0x1',b:'BTC',q:'USDT',k:'PERPETUAL',ts:'0.1',ms:'0.001'}],
  ['grvt.signing.ts', 'GrvtSignature', ['v', 'nonce'], {r:'r',s:'s',v:1,signer:'signer',nonce:1,expiration:'123'}],
  ['grvt.signing.ts', 'OrderPayload', ['nonce'], {subAccountId:'1',isMarket:false,timeInForce:'GTT',postOnly:false,reduceOnly:false,legs:[{instrument:'BTC',size:'1',isBuyingAsset:true}],nonce:1,expiration:'123'}],
  ['grvt.builder.ts', 'GrvtBuilderCheckResult', ['maxFeeRateRaw'], {authorized:true}],
  ['grvt.builder.ts', 'GrvtBuilderInfo', ['builderFeeInt'], {builder:'builder',builderFee:'0.03',builderFeeInt:1}],
];
for (const [relative, name, expected] of models) {
  const file = path.join(directory, relative);
  const source = readTree(file);
  const record = source.statements.find(s => (ts.isInterfaceDeclaration(s) || ts.isTypeAliasDeclaration(s)) && s.name.text === name);
  assert(record, `actual ${name} declaration`);
  const fields = (ts.isInterfaceDeclaration(record) ? record.members : record.type.members).filter(m => m.type.kind === ts.SyntaxKind.NumberKeyword).map(m => m.name.text);
  assert.deepEqual(fields, expected, `complete ${name} numeric inventory`);
}
const spellings = ['-0', '-0.0', '0', '0.0', '1.25', '1e3', '1e400', '-1e400', '-1e-400', '1e-400', '5e-324', '-5e-324', '9007199254740993', '1.7976931348623157e308'];
let state = 0x83ab05c1;
function next() { state ^= state << 13; state ^= state >>> 17; state ^= state << 5; return state >>> 0; }
for (let i = 0; i < 200; i++) spellings.push(`${next() & 1 ? '-' : ''}${next()}.${next()}e${(next() % 1001) - 500}`);
const rows = [];
for (const [, name, fields, base] of models) for (const field of fields) for (const spelling of spellings) {
  const other = {...base}; delete other[field];
  const raw = JSON.stringify(other).slice(0, -1) + `,"${field}":${spelling}}`;
  const value = JSON.parse(raw)[field];
  const bytes = Buffer.alloc(8); bytes.writeDoubleBE(value);
  rows.push({name, field, raw, spelling, bits:bytes.readBigUInt64BE().toString(16).padStart(16, '0')});
}
const file = path.join(directory, 'grvt.types.ts');
const tree = readTree(file);
const message = tree.statements.find(s => s.name?.text === 'GrvtWebSocketMessage');
const error = message.members.find(m => m.name.text === 'error').type;
assert.equal(error.members.find(m => m.name.text === 'code').type.kind, ts.SyntaxKind.NumberKeyword, 'actual message error code is Number');
for (const spelling of spellings) {
  const raw = `{"jsonrpc":"2.0","params":{"n":${spelling}},"result":[${spelling}],"error":{"code":${spelling},"message":"error","data":{"n":${spelling}}}}`;
  const value = JSON.parse(raw).error.code;
  const bytes = Buffer.alloc(8); bytes.writeDoubleBE(value);
  rows.push({name:'GrvtWebSocketMessage',field:'error.code',raw,spelling,bits:bytes.readBigUInt64BE().toString(16).padStart(16,'0')});
}
process.stdout.write(JSON.stringify(rows));
