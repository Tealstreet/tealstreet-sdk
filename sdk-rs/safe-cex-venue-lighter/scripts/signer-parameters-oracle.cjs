const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const assert = require('node:assert/strict');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const file = path.resolve(__dirname, '../../../packages/lighter-sdk/src/client/transaction-client.ts');
const source = fs.readFileSync(file, 'utf8');
const compiled = ts.transpileModule(source, {compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022},reportDiagnostics:true});
assert.equal(compiled.diagnostics.length, 0);
const context = {exports:{},Date:{now:()=>1000}};
vm.runInNewContext(compiled.outputText, context, {filename:file});
const client = new context.exports.TransactionClient({chainId:304,accountIndex:42n,apiKeyIndex:3,privateKey:'',crypto:{},httpClient:{}});
const cases = ['triggerPrice','orderExpiry','expiredAt'].map(field=>({field,value:null}));
for (const field of ['orderType','timeInForce']) for (const value of [null,false,0,1,'','GTC','unknown',{},[]]) cases.push({field,value});
for (const [field,values] of [
 ['orderType',['limit','market','stop_loss','stop_loss_limit','take_profit','take_profit_limit']],
 ['timeInForce',['GoodTillCancel','ImmediateOrCancel','FillOrKill','PostOnly']],
]) for (const value of values) cases.push({field,value});
for (const [index,row] of cases.entries()) {
 const params = {orderId:1n,marketId:1,isBuy:true,price:100n,amount:1n,[row.field]:row.value};
 const value = client.buildCreateOrderData(params,BigInt(index+7));
 row.expected = {data:value.messageHex,expiredAt:String(value.expiredAt),orderTypeValue:String(value.orderTypeValue),
  timeInForceValue:String(value.timeInForceValue),triggerPrice:String(value.triggerPrice),orderExpiry:String(value.orderExpiry)};
}
assert.equal(cases.length,31);
process.stdout.write(JSON.stringify(cases));
