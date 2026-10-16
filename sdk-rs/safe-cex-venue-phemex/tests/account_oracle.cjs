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
const members=owner[0].members.filter(m=>m.name?.getText(ast)==='getAccount');
assert.equal(members.length,1,'actual getAccount body');
const body=ts.transpileModule(`const method=${members[0].initializer.getText(ast)};`,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText;
const cases=[];
for(const raw of [undefined,'null','{}','{"data":null}','{"data":{}}','{"data":{"account":null}}','{"data":{"account":{}}}',
 ...['null','0','-0','false','true','""','"user-23"','23','9007199254740993','1e400','-1e400','1e-400','5e-324','[]','[1,null,"x"]','{}','{"toString":0}','{"valueOf":0}'].map(id=>`{"code":91,"data":{"account":{"userID":${id},"accountID":"different"}}}`),
 '"not-an-object"','[]','{"data":{"account":{"userID":"lowercase", "userId":"wrong-case"}}}']) cases.push({raw:raw??null});
cases.push({raw:'{}',emitFailure:{name:'ListenerError',message:'listener stopped',marker:43}});
cases.push({readFailure:{name:'AxiosError',message:'request failed',marker:67},emitFailure:{name:'Unused',message:'must not emit'}});
(async()=>{
 for(const fixture of cases){
  const events=[];
  const context={ENDPOINTS:constants.ENDPOINTS,xhr:{get:async(url,config)=>{events.push({kind:'read',url,config});if(fixture.readFailure)throw fixture.readFailure;return {data:fixture.raw===null?undefined:JSON.parse(fixture.raw)};}},
   emitAndLogError(message){events.push({kind:'emit',message});if(fixture.emitFailure)throw fixture.emitFailure;}};
  const method=vm.runInNewContext(`${body}\nmethod`,context);
  try {const output=await method(); fixture.expected={ok:true,userId:output.userId,keys:Object.keys(output)};}
  catch(error){fixture.expected={ok:false,name:error.name,message:error.message,readIdentity:error===fixture.readFailure,emitIdentity:error===fixture.emitFailure};}
  fixture.events=events;
 }
 process.stdout.write(JSON.stringify({profile:{node:process.version,major:process.versions.node.split('.')[0],arch:process.arch,platform:process.platform},cases}));
})().catch(error=>{console.error(error);process.exitCode=1});
