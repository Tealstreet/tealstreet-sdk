const assert = require('node:assert/strict'), fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const {root, ts, header, capture, load} = require('./types-oracle.cjs');
const file = path.join(root, 'packages/safe-cex/src/exchanges/weex/weex.utils.ts');
const ast = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
const names = ['toNumber','toIntegerString','normalizeTimestampToMs','normalizeTimestampToSeconds','normalizeOrderSide','normalizeExecution','normalizeHistoricalTrade'];
const nodes = ast.statements.filter(n => ts.isVariableStatement(n) && n.declarationList.declarations.some(d => names.includes(d.name.getText(ast))));
assert.equal(nodes.length, 7, 'fills::source_inventory');
const helper = {exports: {}, require};
const helperFile = path.join(root, 'packages/safe-cex/src/utils/omitUndefined.ts');
vm.runInNewContext(ts.transpileModule(fs.readFileSync(helperFile,'utf8'),{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022,esModuleInterop:true}}).outputText,helper,{filename:helperFile});
const context = {exports: {}, ...load(path.join(root,'packages/safe-cex/src/types.ts')), ...load(path.join(root,'packages/safe-cex/src/exchanges/weex/weex.types.ts')), ...helper.exports};
vm.runInNewContext(ts.transpileModule(nodes.map(n => n.getText(ast)).join('\n'),{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,context,{filename:file});
process.env.TZ = 'UTC';
const market = {id:'venue-market',symbol:'ETHUSDT',base:'ETH',quote:'USDC',active:true,precision:{amount:.001,price:.01},limits:{amount:{min:.1,max:100},leverage:{min:1,max:99}}};
const base = {symbol:'BTCUSDT',side:'BUY',price:31.75,qty:-2.5,time:1700000000123,tradeId:'fill-a',orderId:'order-b'};
const rows = [
 ['distinct',{...base,realizedPnl:-13.25,positionSide:'SHORT',maker:false,commission:-.75,commissionAsset:'BTC',quoteQty:79.375,buyer:true}],
 ['aliases',{symbol:null,orderSide:'sell',price:'17.25',fillSize:'-3',createdTime:'2024-01-02T03:04:05',fillId:99,orderId:11,realizePnl:'5.5',positionSide:'long',fillFee:'-.4',coin:'ETH',buyer:false,maker:true}],
 ['defaults',base],
 ['id_zero',{...base,tradeId:0,fillId:'unused'}],
 ['id_null',{...base,tradeId:null,fillId:null,id:'last-id'}],
 ['id_empty_array',{...base,tradeId:[]}],
 ['order_zero',{...base,orderId:0}],
 ['order_empty_array',{...base,orderId:[]}],
 ['no_order',Object.fromEntries(Object.entries(base).filter(([k])=>k!=='orderId'))],
 ['commission_zero',{...base,commission:0}],
 ['commission_invalid',{...base,commission:'bad'}],
 ['position_unknown',{...base,positionSide:' BOTH ',commissionAsset:null,coin:null}],
 ['nullish',{...base,symbol:null,side:null,orderSide:'SELL',qty:null,fillSize:4,time:null,createdTime:null,updatedTime:1700000000777}],
 ['no_market',{...base,commissionAsset:undefined}],
 ['symbol_empty',{...base,symbol:''}],
 ['side_invalid',{...base,side:'invalid'}],
 ['price_zero',{...base,price:0}],
 ['qty_zero',{...base,qty:0}],
 ['time_invalid',{...base,time:'invalid'}],
 ['null',null],
 ['price_error',{...base,price:{toString:null}}],
 ['time_error',{...base,time:{toString:null}}],
 ['id_error',{...base,tradeId:{toString:null}}],
 ['commission_error',{...base,commission:{toString:null}}],
 ['quote_error',{...base,quoteQty:{toString:null}}],
 ['coercion',{...base,symbol:['A','B'],side:['buy'],price:['19.5'],qty:['-7'],tradeId:[1,2],orderId:[3,4],realizedPnl:['-0']}],
];
const inputs = rows.map(([id,input]) => [id,JSON.stringify(input),id === 'no_market' ? undefined : market]);
inputs.push(['numeric','{"symbol":"BIG","side":"BUY","price":9007199254740993.0,"qty":2,"time":1e18,"tradeId":9007199254740993.0,"orderId":"big-order","realizedPnl":"-0"}',market]);
function run(name, raw, market) {
 try {
  const n = context.exports[name](JSON.parse(raw),market);
  return {result:n===null ? null : Object.fromEntries(Object.entries(n).map(([key,value])=>[key,capture(value)]))};
 } catch(e) {return {error:{name:e.name,message:e.message}};}
}
const cases = inputs.map(([id,raw,market]) => ({id,raw,market:market!==undefined,execution:run('normalizeExecution',raw,market),historical:run('normalizeHistoricalTrade',raw,market)}));
// Malformed optional values survive the actual TS mapper; shared Rust models refuse them.
const shapeInputs = [
 ['maker_null',{...base,maker:null}], ['maker_string',{...base,maker:'false'}],
 ['asset_number',{...base,commissionAsset:7}], ['buyer_null',{...base,buyer:null}],
];
const shapeRefusals = shapeInputs.map(([id,input]) => ({id,raw:JSON.stringify(input),execution:run('normalizeExecution',JSON.stringify(input),market),historical:run('normalizeHistoricalTrade',JSON.stringify(input),market)}));
const callable = context.exports.normalizeExecution({...base,price:{toString:()=> '7.25'}},market);
assert.equal(callable.price,7.25);
const utf16 = context.exports.normalizeExecution({...base,symbol:'\ud800'},market);
assert.equal(utf16.symbol.charCodeAt(0),0xd800);
process.stdout.write(JSON.stringify({header,market,cases,shapeRefusals,callable:capture(callable.price)}));
