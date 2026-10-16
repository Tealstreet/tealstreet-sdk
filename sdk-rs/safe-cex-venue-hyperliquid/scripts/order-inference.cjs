const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const root = path.resolve(__dirname, '../../..');
const source = fs.readFileSync(path.join(root, 'packages/safe-cex/src/exchanges/hyperliquid/hyperliquid.order-inference.ts'), 'utf8');
const typesFile = path.join(root, 'packages/safe-cex/src/types.ts');
const types = ts.createSourceFile(typesFile, fs.readFileSync(typesFile, 'utf8'), ts.ScriptTarget.Latest, true);
const declarations = types.statements.filter(node => ts.isEnumDeclaration(node) && ['OrderType', 'OrderTimeInForce'].includes(node.name.text));
assert.equal(declarations.length, 2, 'actual shared inference enums');
const compilerOptions = {module:ts.ModuleKind.CommonJS, target:ts.ScriptTarget.ES2022};
const shared = {};
vm.runInNewContext(ts.transpileModule(declarations.map(node => node.getText(types)).join('\n'), {compilerOptions}).outputText, {exports:shared});
const exportsOut = {};
vm.runInNewContext(ts.transpileModule(source, {compilerOptions}).outputText + '\nexports.number = toNumberOrZero;', {exports:exportsOut, require:name => {assert.equal(name, '../../types'); return shared;}});
const bits = value => {const bytes = Buffer.alloc(8); bytes.writeDoubleBE(value); return bytes.toString('hex');};
const cases = [];
function capture(operation, raw) {
  const row = {operation, raw};
  if (raw.includes('\\ud800') || raw.includes('\\udfff')) row.representationRefusal = true;
  try {
    const input = JSON.parse(raw);
    if (operation === 'infer') {
      const value = exportsOut.inferHyperliquidOrderTypeAndPrice(input);
      row.type = value.type; row.priceBits = bits(value.price); row.source = value.source;
    } else if (operation === 'number') row.bits = bits(exportsOut.number(input));
    else row.tif = exportsOut.parseHyperliquidOrderTimeInForce(input) ?? null;
  } catch (error) {row.errorName = error.name; row.error = error.message;}
  cases.push(row);
}
const orders = [
  {}, {orderType:'Limit',limitPx:'91.25',triggerPx:'17.5',tpsl:'tp'},
  {orderType:'Take Profit Market',triggerPx:'17.5',limitPx:'91.25'},
  {orderType:'Stop Market',triggerPx:'23.75',limitPx:'91.25'},
  {triggerPx:'31.125',tpsl:'tp'}, {triggerPx:'37.25',tpsl:'sl'},
  {triggerPx:'41.5'}, {limitPx:'47.75'},
  {orderType:'unrecognized',triggerPx:'53.125',tpsl:'tp'},
  {orderType:'unrecognized',triggerPx:'59.25',tpsl:'sl'},
  {orderType:'Stop Market',triggerPx:'61.5',tpsl:'tp'},
  {orderType:'Take Profit Market',triggerPx:0,tpsl:'sl'},
  {triggerPx:0,tpsl:'tp',limitPx:'67.25'}, {triggerPx:'0',tpsl:'tp'},
  {triggerPx:false,limitPx:false}, {triggerPx:true,tpsl:'sl'},
  {triggerPx:[],tpsl:'tp'}, {triggerPx:{},tpsl:'sl'},
  {orderType:['Limit'],limitPx:'73.125'}, {orderType:0,triggerPx:'79.25',tpsl:['tp']},
  {orderType:'Limit',limitPx:'-0'}, {orderType:'Limit',limitPx:'1e400'},
  {orderType:'Limit',limitPx:'\ufeff12tail'}, {orderType:'Limit',limitPx:'\u008512'},
  {orderType:'limit'}, {orderType:'Limit',tpsl:{toString:null},limitPx:83},
  {orderType:{toString:null},triggerPx:{toString:null}},
  {triggerPx:{toString:null},tpsl:'tp'}, {orderType:'Limit',limitPx:{toString:null}},
];
for (const order of orders) capture('infer', JSON.stringify(order));
for (const raw of ['null', '[]', '123', '{"orderType":"\\ud800","triggerPx":"12","tpsl":"tp"}', '{"orderType":"Limit","limitPx":"\\udfff"}']) capture('infer', raw);
for (const raw of ['null', '0', '-0', '"-0"', '"12abc"', '"0x10"', '"1e400"', '1e400', 'true', '[]', '[12,13]', '"\\ufeff12tail"', '"\\u008512"', '{"toString":null}', '"\\ud800"', '"😀12"']) capture('number', raw);
for (const raw of ['"Alo"', '"Gtc"', '"Ioc"', '"alo"', '"ALO"', '" Gtc"', 'null', 'false', '0', '[]', '["Alo"]', '{}']) capture('tif', raw);
assert.equal(cases.length, 62, 'inference fixed source case inventory');
console.log(JSON.stringify({source:'actual order-inference module and actual shared enum declarations; owned JSON inputs, native bits before serialization',cases}));
