const fs = require('node:fs');
const path = require('node:path');
const assert = require('node:assert/strict');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const root = path.resolve(__dirname, '../../../packages/lighter-sdk/src/types');
const schemas = {};
for (const file of ['common.ts', 'orders.ts']) {
  const tree = ts.createSourceFile(file, fs.readFileSync(path.join(root, file), 'utf8'), ts.ScriptTarget.Latest, true);
  for (const node of tree.statements.filter(ts.isInterfaceDeclaration)) {
    if (!['Account', 'OrderBook', 'PaginatedResponse', 'BatchCancelOrdersParams'].includes(node.name.text)) continue;
    assert.ok(!node.heritageClauses?.length, `${node.name.text} unsupported interface heritage`);
    schemas[node.name.text] = Object.fromEntries(node.members.map(f => [f.name.text, f.type.getText(tree)]));
  }
}
for (const [name, field, type] of [
  ['Account', 'balances', 'Balance[]'], ['OrderBook', 'bids', 'OrderBookLevel[]'],
  ['OrderBook', 'asks', 'OrderBookLevel[]'], ['PaginatedResponse', 'items', 'T[]'],
  ['BatchCancelOrdersParams', 'orderIds', 'string[]'], ['BatchCancelOrdersParams', 'clientOrderIds', 'string[]'],
]) assert.equal(schemas[name][field], type, `${name}.${field} source collection type`);
const balance = (symbol, decimals) => ({symbol, tokenAddress:`token:${symbol}`, available:'1', locked:'2', total:'3', decimals});
const cases = {
  account: {address:'address', accountIndex:1, publicKey:'key', nonce:2, balances:[balance('A', 3.25), balance('B', 4.5)], createdAt:5, updatedAt:6},
  book: {symbol:'book', bids:[{price:'11',quantity:'12'},{price:'13',quantity:'14'}], asks:[{price:'21',quantity:'22'},{price:'23',quantity:'24'}], lastUpdateId:7, timestamp:8},
  page: {items:[{value:31.25},{value:32.5}],total:2,page:1,pageSize:2,totalPages:1,hasNext:false,hasPrev:true},
  cancel: {orderIds:['order-B','order-A'],clientOrderIds:['client-B','client-A'],symbol:'cancel',accountIndex:9,apiKeyIndex:10},
};
process.stdout.write(JSON.stringify({header:{nodeVersion:process.version,captureArchitecture:process.arch,capturePlatform:process.platform,consumer:"live_sdk_collections",scope:"later collection-cardinality supplement"},schemas,cases}));
