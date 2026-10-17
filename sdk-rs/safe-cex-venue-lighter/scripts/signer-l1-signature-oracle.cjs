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
 const cases=[];
 for(const label of labels) {
  const raw=input(label);
  const params={integratorAccountIndex:8n,maxPerpsTakerFee:250,maxPerpsMakerFee:0,maxSpotTakerFee:300,maxSpotMakerFee:0,approvalExpiry:1000000n,l1Signature:raw};
  const tx=await client.getApproveIntegratorTransaction(params);
  const value=tx.approveIntegratorDetails.l1Signature;
  cases.push({label,data:tx.data,retained:Object.is(value,raw),type:typeof value,
   negativeZero:typeof value==='number'&&Object.is(value,-0),nan:typeof value==='number'&&Number.isNaN(value)});
 }
 assert.equal(cases.length,17);assert.equal(nonceReads,1);
 process.stdout.write(JSON.stringify(cases),()=>process.exit(0));
}
main().catch(error=>{console.error(error);process.exit(1);});
