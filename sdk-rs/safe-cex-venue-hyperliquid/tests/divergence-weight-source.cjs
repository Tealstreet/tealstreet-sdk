'use strict';
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH),root=path.resolve(__dirname,'../../..');
const directory=path.join(root,'packages/safe-cex/src/exchanges/hyperliquid');
const compile=text=>ts.transpileModule(text,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
const types=fs.readFileSync(path.join(directory,'hyperliquid.types.ts'),'utf8');
const tree=ts.createSourceFile('hyperliquid.types.ts',types,ts.ScriptTarget.Latest,true);
const selected=tree.statements.filter(node=>ts.isVariableStatement(node)&&node.declarationList.declarations.some(item=>['ENDPOINTS','INFO_TYPES'].includes(item.name.text)));
assert.equal(selected.length,2,'SOURCE two maintained endpoint/type declarations');
const constants={};vm.runInNewContext(compile(selected.map(node=>node.getText(tree)).join('\n')),{exports:constants});
const sleeps=[];let now=0;
const moduleExports={};
vm.runInNewContext(compile(fs.readFileSync(path.join(directory,'hyperliquid.rateLimiter.ts'),'utf8')),{exports:moduleExports,require:name=>{assert.equal(name,'./hyperliquid.types');return constants;},Date:{now:()=>now},setTimeout:(resolve,delay)=>{sleeps.push(delay);now+=delay;resolve();}});
(async()=>{
  const weight=moduleExports.getHyperliquidRestRequestWeight;
  const cases=[
    ['/info','{"type":"userRole","unused":"\\ud800"}',60],
    ['/info','{"type":"userRole","unused":'+'['.repeat(140)+'0'+']'.repeat(140)+'}',60],
    ['/info','{"type":"candleSnapshot","limit":{"$serde_json::private::Number":"120"}}',20],
    ['/info','"{\\"type\\":\\"userRole\\"}"',20],
    ['/info','{"type":"userRole","limit":"\\ud800"}',60],
    ['/exchange','{"action":{"type":"order","orders":['+Array(40).fill('"\\ud800"').join(',')+']}}',2],
  ];
  for(const [url,data,expected] of cases) assert.equal(weight({url,data}),expected,'SOURCE native raw request weight');
  for(const raw of ['{"type":"userRole","limit":["\\ud800",{"toString":0}]}','{"type":"not-extra-weighted","limit":{"toString":0}}']) {
    assert.throws(()=>weight({url:'/info',data:raw}),error=>error.name==='TypeError'&&error.message==='Cannot convert object to primitive value','SOURCE consumed limit poison even after an invalid unit');
  }
  const apiText=fs.readFileSync(path.join(directory,'hyperliquid.api.ts'),'utf8');
  const apiTree=ts.createSourceFile('hyperliquid.api.ts',apiText,ts.ScriptTarget.Latest,true);
  const interceptor=apiTree.statements.filter(node=>ts.isVariableStatement(node)&&node.declarationList.declarations.some(item=>item.name.text==='addRateLimitInterceptor'));
  assert.equal(interceptor.length,1);
  let callback,waitCalls=0;
  const context={exports:{},getHyperliquidRestRequestWeight:weight,hyperliquidRestRateLimiter:{waitForToken(){waitCalls++;}},isSigningCaptureRequest:()=>false};
  vm.runInNewContext(compile(interceptor[0].getText(apiTree)+'\nexports.install=addRateLimitInterceptor;'),context);
  context.exports.install({interceptors:{request:{use:fn=>{callback=fn;}}}});
  await assert.rejects(()=>callback({url:'/info',data:{type:'candleSnapshot',limit:{toString:0}}}),error=>error.name==='TypeError'&&error.message==='Cannot convert object to primitive value','SOURCE interceptor preserves Number error class and message');
  assert.equal(waitCalls,0,'SOURCE weight error precedes limiter');
  const limiter=new moduleExports.RateLimiter();
  await limiter.waitForToken();await limiter.waitForToken(1);
  assert.deepEqual(sleeps,[50],'SOURCE omitted weight reserves one token');
  assert.equal(cases.length,6);
  process.stdout.write(JSON.stringify({cases:10,rawWeights:6,poison:2,default:1,interceptorError:1,last:'omitted-default'})+'\n');
})().catch(error=>{console.error(error);process.exitCode=1;});
