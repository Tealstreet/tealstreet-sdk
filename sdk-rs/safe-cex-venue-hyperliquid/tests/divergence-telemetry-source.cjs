'use strict';
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const root=path.resolve(__dirname,'../../..');
const source=name=>fs.readFileSync(path.join(root,'packages/safe-cex/src/exchanges/hyperliquid',name),'utf8');
const api=source('hyperliquid.api.ts');
const tree=ts.createSourceFile('hyperliquid.api.ts',api,ts.ScriptTarget.Latest,true);
const selected=['getRequestActionType','getOrderErrorTelemetry','addResponseInterceptor'].map(name=>{
  const matches=tree.statements.filter(node=>ts.isVariableStatement(node)&&node.declarationList.declarations.some(item=>item.name.text===name));
  assert.equal(matches.length,1,'one maintained API declaration '+name);return matches[0].getText(tree);
}).join('\n');
const types=source('hyperliquid.types.ts');
const typeTree=ts.createSourceFile('hyperliquid.types.ts',types,ts.ScriptTarget.Latest,true);
const endpoints=typeTree.statements.filter(node=>ts.isVariableStatement(node)&&node.declarationList.declarations.some(item=>item.name.text==='ENDPOINTS'));
assert.equal(endpoints.length,1);
const program=source('hyperliquid.order-response.ts')+'\n'+endpoints[0].getText(typeTree)+'\n'+selected+'\nexports.install=addResponseInterceptor;';
const context={exports:{},describeHttpFailure:()=>assert.fail('unused HTTP diagnostic branch'),isSigningCaptureRequest:()=>false};
vm.runInNewContext(ts.transpileModule(program,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText,context);
(async()=>{
  const raw='{"action":{"type":"order"},"unused":{"$serde_json::private::Number":"120"}}';
  let completed=0;
  for(const input of [JSON.parse(raw),raw]) {
    let callback,providerCalls=0,emissions=0;
    const nativeContext={category:'order'};
    const exchange={getOrderErrorTelemetry(data,message){
      assert.equal(data,input,'SOURCE telemetry sees original data value/reference');
      assert.equal(typeof data,typeof input,'SOURCE telemetry data phase unchanged');
      assert.equal(message,'rejected');providerCalls++;return nativeContext;
    },emitAndLogError(message,unused,context){
      assert.equal(message,'rejected');assert.equal(unused,undefined);assert.equal(context,nativeContext);emissions++;
    }};
    context.exports.install({interceptors:{response:{use:fn=>{callback=fn;}}}},exchange);
    await assert.rejects(()=>callback({status:200,data:{error:'rejected'},config:{url:'/exchange',data:input}}),error=>error.message==='rejected');
    assert.equal(providerCalls,1);assert.equal(emissions,1);completed++;
  }
  assert.equal(completed,2,'SOURCE independent object/string telemetry phase floor');
  process.stdout.write(JSON.stringify({cases:2,last:'string',nativeAssertions:true})+'\n');
})().catch(error=>{console.error(error);process.exitCode=1;});
