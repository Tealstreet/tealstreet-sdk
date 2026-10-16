const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const assert = require('node:assert/strict');
assert(process.env.SAFE_CEX_TYPESCRIPT_PATH, 'SAFE_CEX_TYPESCRIPT_PATH required');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const root = path.resolve(__dirname, '../../..');
function execute(relative, imports = {}) {
 const filename = path.join(root,relative);
 const code = ts.transpileModule(fs.readFileSync(filename,'utf8'), {compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText;
 const module = {exports:{}};
 vm.runInThisContext(`(function(require,module,exports){${code}\n})`,{filename})(name=>{assert(Object.hasOwn(imports,name),`unexpected import ${name}`);return imports[name]},module,module.exports);
 return module.exports;
}
const shared = execute('packages/safe-cex/src/types.ts', {'@tealstreet/logging/src/LogSeverity':execute('packages/logging/src/LogSeverity.ts')});
const constants = execute('packages/safe-cex/src/exchanges/phemex/phemex.types.ts', {'../../types':shared});
const filename = path.join(root,'packages/safe-cex/src/exchanges/phemex/phemex.exchange.ts');
const ast = ts.createSourceFile(filename,fs.readFileSync(filename,'utf8'),ts.ScriptTarget.ES2022,true);
const owner = ast.statements.filter(s=>ts.isClassDeclaration(s)&&s.name.text==='PhemexExchange');
assert.equal(owner.length,1,'actual PhemexExchange declaration');
const members=owner[0].members.filter(m=>m.name?.getText(ast)==='validateAccount');
assert.equal(members.length,1,'actual validateAccount body');
const body=ts.transpileModule(`const method=${members[0].initializer.getText(ast)};`,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText;
function capture(value) {
 if(value===undefined)return {kind:'undefined'};
 if(value===null)return {kind:'null'};
 if(typeof value==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(value);return {kind:'number',bits:Number.isNaN(value)?'NaN':b.toString('hex')};}
 if(Array.isArray(value))return {kind:'array',items:value.map(capture)};
 if(typeof value==='object')return {kind:'object',entries:Object.entries(value).map(([k,v])=>[k,capture(v)])};
 return {kind:typeof value,value};
}
const cases=[];
const add=(label,raw)=>cases.push({label,raw:raw??null});
for(const raw of [undefined,'null','{}','[]','false','{"data":null}','{"data":{"account":{}}}','{"data":{"account":23}}'])add('missing-'+cases.length,raw);
for(const total of ['"127.75units"','0','-0','" -0suffix"','"1e309"','1e400','null','true','[]','[12,3]','{}','{"toString":0}','"0x10"','"1e-400"','9007199254740993','"1e2bad"','"\\uFEFF 17.5"'])add('total-'+total,`{"code":91,"data":{"account":{"accountBalanceRv":${total},"totalUsedBalanceRv":"23.5tail"}}}`);
for(const used of ['null','false','0','-0','"-0"','"nonsense"','"12.25tail"','"Infinity"','"-Infinity"','1e400','[13,2]','{}','{"toString":0}'])add('used-'+used,`{"data":{"account":{"accountBalanceRv":"127.75","totalUsedBalanceRv":${used}}}}`);
add('invalid total skips used coercion','{"data":{"account":{"accountBalanceRv":"bad","totalUsedBalanceRv":{"toString":0}}}}');
add('finite total missing used','{"data":{"account":{"accountBalanceRv":"19.5"}}}');
for(const error of ['null','0','"failure"','{}','{"message":"fallback"}','{"message":false}',
 '{"response":{"data":{"message":"emitted","msg":"returned"}},"message":"outer"}',
 '{"response":{"data":{"message":0,"msg":false}},"message":"outer"}',
 '{"response":{"data":{"message":[],"msg":{}}},"message":"outer"}',
 '{"response":{"data":{"message":1e400,"msg":-0}},"message":23}',
 '{"response":{"data":{"message":"only emitted"}}}',
 '{"response":{"data":{"msg":"only returned"}}}',
 '{"response":{"data":{"msg":1e400}},"message":"outer"}',
 '{"response":{"data":{"msg":9007199254740993}},"message":"outer"}',
 '{"response":{"data":{"msg":[]}},"message":"outer"}',
 '{"response":null,"message":[]}',
 '{"response":{"data":null},"message":{}}',
 '{"response":{"data":{"message":false,"msg":0}},"message":""}'])cases.push({label:'error-'+cases.length,error});
cases.push({label:'nonempty object errors',error:'{"response":{"data":{"message":{"alpha":7,"beta":11},"msg":{"code":19,"detail":"denied"}}}}'});
cases.push({label:'nonempty array errors',error:'{"response":{"data":{"message":["first","second"],"msg":["returned",23,"tail"]}}}'});
cases.push({label:'emit read failure' ,error:'{"message":"read error"}',emitFailure:{name:'ListenerError',message:'listener stopped',marker:17}});
cases.push({label:'emit coercion failure',raw:'{"data":{"account":{"accountBalanceRv":{"toString":0}}}}',emitFailure:{name:'ListenerError',message:'coercion listener stopped',marker:29}});
(async()=>{
 for(const fixture of cases){
  const events=[];
  const context={ENDPOINTS:constants.ENDPOINTS,xhr:{get:async(url,config)=>{events.push({kind:'read',url,config});if(Object.hasOwn(fixture,'error'))throw JSON.parse(fixture.error);return {data:fixture.raw===null?undefined:JSON.parse(fixture.raw)};}},
   emitAndLogError(message){events.push({kind:'emit',value:capture(message)});if(fixture.emitFailure)throw fixture.emitFailure;}};
  const method=vm.runInNewContext(`${body}\nmethod`,context);
  try {const output=await method();fixture.expected={ok:true,error:capture(output.error),balance:capture(output.balance)};}
  catch(error){fixture.expected={ok:false,emitIdentity:error===fixture.emitFailure};}
  fixture.events=events;
 }
 process.stdout.write(JSON.stringify({profile:{node:process.version,major:process.versions.node.split('.')[0],arch:process.arch,platform:process.platform},cases}));
})().catch(error=>{console.error(error);process.exitCode=1});
