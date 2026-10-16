const fs = require('node:fs');
const path = require('node:path');
const assert = require('node:assert/strict');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const file = path.resolve(__dirname, '../../../packages/lighter-sdk/src/types/orders.ts');
let source = fs.readFileSync(file, 'utf8');
const mutation = process.env.LIGHTER_SDK_ORDERS_MUTATION;
if (mutation === 'inherited-number') source = source.replace(/(interface CancelOrderParams(?:<[^>]+>)?)(\s*\{)/, '$1 extends ExtraNumeric$2') + '\ninterface ExtraNumeric { hiddenNumeric: number; }\n';
if (mutation === 'required-order-id') source = source.replace('orderId?: string;', 'orderId: string;');
if (mutation === 'stats-string') source = source.replace('totalOrders: number;', 'totalOrders: string;');
if (mutation === 'order-type-string') source = source.replace('type: OrderType;', 'type: string;');
if (mutation === 'reduce-only-required') source = source.replace('reduceOnly?: boolean;', 'reduceOnly: boolean;');
if (mutation === 'cancel-array-scalar') source = source.replace('orderIds?: string[];', 'orderIds?: string;');
const stringKind = kind => kind === 'string' || kind === 'string[]';
const numericKind = kind => ['number','OrderType','OrderSide','OrderStatus','TimeInForce','TriggerStatus'].includes(kind);
const tree = ts.createSourceFile(file, source, ts.ScriptTarget.Latest, true);
const schemas = {}, complete = {}, numeric = {}, strings = {};
const bits = n => { const b=Buffer.alloc(8);b.writeDoubleBE(n);return b.toString('hex'); };
for (const name of ['CancelOrderParams','CancelAllOrdersParams','QueryFillsParams','OrderStats','OrderInfo','Fill','CreateLimitOrderParams','CreateMarketOrderParams','CreateStopLossOrderParams','CreateTakeProfitOrderParams','QueryOrdersParams','ModifyOrderParams','BatchCancelOrdersParams']) {
  const node = tree.statements.find(n => ts.isInterfaceDeclaration(n) && n.name.text === name);
  assert.ok(node, name);
  assert.ok(!node.heritageClauses?.length, `${name} unsupported interface heritage`);
  schemas[name] = node.members.map(f=>({name:f.name.text,type:f.type.getText(tree),optional:!!f.questionToken}));
  complete[name] = Object.fromEntries(schemas[name].map(f=>[f.name,numericKind(f.type)?16777217.25:f.type==='boolean'?true:f.type==='string[]'?[`${name}.${f.name}.b`,`${name}.${f.name}.a`,`${name}.${f.name}.b`,`${name}.${f.name}.b`]:`${name}.${f.name}`]));
  numeric[name] = schemas[name].filter(f=>numericKind(f.type)).flatMap(f => ['-0','-0.0','-0e0','0','1e3','1e400','-1e400','16777217.25'].map(raw=> {
    const value={...complete[name],[f.name]:'__NUMBER__'};
    const encoded=JSON.stringify(value).replace('"__NUMBER__"',raw);
    return {field:f.name,raw,encoded,bits:bits(JSON.parse(encoded)[f.name])};
  }));
  strings[name] = ['','plain','é','👩','\ufeff','\u0085','\ud800','\udfff','x\ud800y'].map(text=>{
    const value={...complete[name]};
    for (const f of schemas[name].filter(f=>stringKind(f.type))) value[f.name]=f.type==='string[]'?[text,text]:text;
    const encoded=JSON.stringify(value), parsed=JSON.parse(encoded);
    for (const f of schemas[name].filter(f=>stringKind(f.type))) for (const item of (f.type==='string[]'?parsed[f.name]:[parsed[f.name]])) assert.equal(item,text);
    return {encoded,units:Array.from({length:text.length},(_,i)=>text.charCodeAt(i)),representable:text.isWellFormed()};
  });
}
process.stdout.write(JSON.stringify({schemas,complete,numeric,strings}));
