const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const assert = require('node:assert/strict');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const root = path.resolve(__dirname, '../../..');
const labels = ['undefined','null','false','true','zero','one','empty','text','array','object','nan','infinity','negative-zero','big-zero','big-one','symbol','function'];
function input(label) {
 return {undefined:()=>undefined,null:()=>null,false:()=>false,true:()=>true,zero:()=>0,one:()=>1,empty:()=>'',text:()=>'yes',
  array:()=>[],object:()=>({}),nan:()=>NaN,infinity:()=>Infinity,'negative-zero':()=>-0,'big-zero':()=>0n,'big-one':()=>1n,
  symbol:()=>Symbol.iterator,function:()=>function input(){}}[label]();
}
function observe(tx, field, value) {
 const raw = tx.orderDetails[field];
 return {data:tx.data,orderType:String(tx.orderDetails.orderTypeValue),timeInForce:String(tx.orderDetails.timeInForceValue),
  retained:Object.is(raw,value),type:typeof raw,negativeZero:typeof raw==='number'&&Object.is(raw,-0),nan:typeof raw==='number'&&Number.isNaN(raw)};
}
async function main() {
 const wasmContext = vm.createContext({TextEncoder,TextDecoder,WebAssembly,Buffer,performance,setTimeout,clearTimeout,
  console:{log(){},error:console.error},crypto:{getRandomValues(bytes){bytes.fill(0x42);return bytes;}}});
 vm.runInContext(fs.readFileSync(path.join(root,'packages/poseidon-crypto/src/wasm/wasm_exec.js'),'utf8'),wasmContext);
 const go = new wasmContext.Go();
 const instance = await WebAssembly.instantiate(fs.readFileSync(path.join(root,'packages/poseidon-crypto/src/wasm/poseidon_crypto.wasm')),go.importObject);
 go.run(instance.instance);
 const crypto = {isReady:()=>typeof wasmContext.schnorrSign==='function'};
 for(const name of ['poseidon2Hash','poseidon2HashFp5','poseidon2HashTwoToOne','schnorrSign']) {
  assert.equal(typeof wasmContext[name],'function');
  crypto[name]=(...args)=>wasmContext[name](...args);
 }
 const file=path.join(root,'packages/lighter-sdk/src/client/transaction-client.ts');
 const source=fs.readFileSync(file,'utf8');
 const compiled=ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022},reportDiagnostics:true});
 assert.equal(compiled.diagnostics.length,0);
 const context={exports:{},Date:{now:()=>1000},Buffer,TextEncoder};
 vm.runInNewContext(compiled.outputText,context,{filename:file});
 let nonceReads=0;
 const client=new context.exports.TransactionClient({chainId:304,accountIndex:42n,apiKeyIndex:3,privateKey:'01'+'00'.repeat(39),crypto,
  httpClient:{async getNextNonce(){nonceReads++;return 7n;}}});
 const order=()=>({orderId:1n,marketId:1,isBuy:true,price:100n,amount:1n,orderType:'limit'});
 const singles=[];
 for(const field of ['isBuy','postOnly','reduceOnly']) for(const label of labels) {
  const value=input(label),params={...order(),[field]:value};
  const tx=await client.getCreateOrderTransaction(params);
  assert.equal(typeof tx.signature,'string');
  singles.push({field,label,expected:observe(tx,field,value)});
 }
 const batchInput=[{...order(),orderId:2n,isBuy:0},{...order(),orderId:3n,postOnly:'yes'},{...order(),orderId:4n,reduceOnly:{}}];
 const batch=(await client.getCreateOrderTransactions(batchInput)).map((tx,i)=>observe(tx,['isBuy','postOnly','reduceOnly'][i],batchInput[i][['isBuy','postOnly','reduceOnly'][i]]));
 const groupedInput={...order(),orderId:5n,isBuy:0,reduceOnly:{}};
 const child={...order(),orderId:6n,isBuy:true,orderType:'stop_loss',amount:0n,reduceOnly:true};
 const grouped=await client.getCreateGroupedOrdersTransaction({groupingType:1,orders:[groupedInput,child]});
 const detail=grouped.groupedOrderDetails.orders[0];
 const group={data:grouped.data,isBuyRetained:Object.is(detail.isBuy,groupedInput.isBuy),reduceOnlyRetained:detail.reduceOnly===groupedInput.reduceOnly};
 const sideCases=[];
 for(const [label,groupingType,sideA,sideB] of [['falsey-oto',1,0,false],['objects-oto',1,[],[]],['falsey-oco',2,0,false]]) {
  const a={...order(),orderId:8n,isBuy:sideA,reduceOnly:true};
  const b={...order(),orderId:9n,isBuy:sideB,reduceOnly:true,orderType:'stop_loss',amount:0n};
  if(groupingType===2){a.orderType='take_profit';b.amount=1n;}
  try {
   const tx=await client.getCreateGroupedOrdersTransaction({groupingType,orders:[a,b]});
   const rows=tx.groupedOrderDetails.orders;
   sideCases.push({label,data:tx.data,retained:rows[0].isBuy===sideA&&rows[1].isBuy===sideB});
  }catch(error){sideCases.push({label,error:error.message});}
 }
 assert.equal(sideCases.length,3);
 assert.equal(singles.length,51);assert.equal(batch.length,3);assert.equal(nonceReads,1);
 process.stdout.write(JSON.stringify({singles,batch,group,sideCases}),()=>process.exit(0));
}
main().catch(error=>{console.error(error);process.exit(1);});
