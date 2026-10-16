const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const source=fs.readFileSync(path.resolve(__dirname,'../../../packages/safe-cex/src/exchanges/hyperliquid/hyperliquid.order-response.ts'),'utf8');
const out={};vm.runInNewContext(ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,{exports:out,Set});
const cases=[];for(const statuses of [['waitingForFill'],['waitingForTrigger'],[{resting:{oid:101}},{filled:{oid:202}},'waitingForFill'],[{resting:{oid:301}},{filled:{oid:402}},'waitingForTrigger']]) {
 const input={response:{data:{statuses}}};cases.push({raw:JSON.stringify(input),waiting:out.hyperliquidOrderResponseHasWaitingForTrigger(input),deferred:out.hyperliquidOrderResponseHasDeferredOrderStatus(input),parsed:out.parseHyperliquidOrderResponse(input)});
}
const output=JSON.stringify({label:'Self-authored API field-identity supplement, actual source; original api.json unchanged',cases},null,2)+'\n',target=path.join(__dirname,'../tests/fixtures/api-field-identities.json');if(process.argv.includes('--capture'))fs.writeFileSync(target,output,{flag:'wx'});else assert.equal(output,fs.readFileSync(target,'utf8'));console.log(cases.length+' API identity cases');
