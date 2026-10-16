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
const members=owner[0].members.filter(m=>m.name?.getText(ast)==='fetchTickersRaw');
assert.equal(members.length,1,'actual fetchTickersRaw body');
const body=ts.transpileModule(`const method=${members[0].initializer.getText(ast)};`,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText;
const getKey=execute('packages/safe-cex/src/utils/get-key.ts', Object.fromEntries(['camelCase','snakeCase','memoize'].map(name=>['lodash/'+name,require('lodash/'+name)])));
function capture(value) {
 if(value===undefined)return {kind:'undefined'};
 if(value===null)return {kind:'null'};
 if(typeof value==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(value);return {kind:'number',bits:Number.isNaN(value)?'NaN':b.toString('hex')};}
 if(Array.isArray(value))return {kind:'array',items:value.map(capture)};
 if(typeof value==='object')return {kind:'object',entries:Object.entries(value).map(([key,v])=>[key,capture(v)])};
 return {kind:typeof value,value};
}
const cases=[undefined,'null','{}','[]','0','-0','false','true','""','"bad"','1e400',
 ...['null','false','0','""','"venue-error"','{}','[]'].map(error=>`{"error":${error},"result":{"bid":12.3,"ask":45.6}}`),
 '{"error":null,"result":{"zero":-0,"overflow":1e400,"rounded":9007199254740993,"tiny":5e-324}}',
 '{"Error":null,"result":[1,2]}'].map(raw=>({raw:raw??null}));
cases.push({raw:'{}',emitFailure:{name:'ListenerError',message:'emitter stopped',marker:47}});
cases.push({raw:'{"error":"fail"}',emitFailure:{name:'BracketListener',message:'emitter rejected',marker:53}});
cases.push({readFailure:{name:'AxiosError',message:'request failed',marker:71}});
(async()=>{
 for(const fixture of cases){
  const events=[];const input=fixture.raw===null?undefined:fixture.raw===undefined?undefined:JSON.parse(fixture.raw);
  const context={ENDPOINTS:constants.ENDPOINTS,v:getKey.v,xhr:{get:async(...args)=>{events.push({kind:'read',args});if(fixture.readFailure)throw fixture.readFailure;return {data:input};}},
   emitAndLogError(value){events.push({kind:'emit',value:capture(value)});if(fixture.emitFailure)throw fixture.emitFailure;}};
  const method=vm.runInNewContext(`${body}\nmethod`,context);
  try {const value=await method();fixture.expected={ok:true,value:capture(value),sameInput:value===input};}
  catch(error){fixture.expected={ok:false,name:error.name,message:error.message,readIdentity:error===fixture.readFailure,emitIdentity:error===fixture.emitFailure};}
  fixture.events=events;
 }
 process.stdout.write(JSON.stringify({profile:{node:process.version,major:process.versions.node.split('.')[0],arch:process.arch,platform:process.platform},cases}));
})().catch(error=>{console.error(error);process.exitCode=1});
