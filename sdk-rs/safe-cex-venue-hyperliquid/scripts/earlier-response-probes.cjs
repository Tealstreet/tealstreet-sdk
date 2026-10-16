// Actual earlier parser source; one process captures all adversarial coercion cases.
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const source=fs.readFileSync(path.resolve(__dirname,'../../../packages/safe-cex/src/exchanges/hyperliquid/hyperliquid.order-response.ts'),'utf8');
const out={};vm.runInNewContext(ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,{exports:out,Set});
const cases=[];for(const oid of [{toString:null},{toString:123},{toString:{},valueOf:1},[1,{toString:null}],{valueOf:1},101,'202']) {
 const data={response:{data:{statuses:[{resting:{oid}}]}}},raw=JSON.stringify(data);let result,error;try{result=out.parseHyperliquidOrderResponse(data);}catch(e){error={name:e.name,message:e.message}}
 cases.push({raw,...(error?{error}:{result})});
}
for(const status of [{resting:{oid:{toString:null}},filled:{oid:202}},{resting:{oid:101},filled:{oid:{toString:null}}},{resting:{oid:null},filled:{oid:{toString:null}}}]) {
 const data={response:{data:{statuses:[status]}}},raw=JSON.stringify(data);let result,error;try{result=out.parseHyperliquidOrderResponse(data);}catch(e){error={name:e.name,message:e.message}}cases.push({raw,...(error?{error}:{result})});
}
assert(cases[0].error,'non-callable toString must throw in actual source');
process.stdout.write(JSON.stringify({source:'actual order-response parser; old ported getOid and parse caller',cases})+'\n');
