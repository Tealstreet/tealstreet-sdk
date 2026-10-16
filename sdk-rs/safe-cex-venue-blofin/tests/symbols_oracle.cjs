// Execute the actual getter initializers against the actual maintained DefaultStore.
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
assert(process.env.SAFE_CEX_TYPESCRIPT_PATH,'compiler_required');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const root=path.resolve(__dirname,'../../..');
const external=require('node:module').createRequire(require.resolve(process.env.SAFE_CEX_TYPESCRIPT_PATH));
const context=vm.createContext({structuredClone,process,setTimeout,clearTimeout,console});
const cache=new Map();
function execute(file) {
  if(cache.has(file))return cache.get(file);
  const exports={};cache.set(file,exports);
  const output=ts.transpileModule(fs.readFileSync(file,'utf8'),{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022,esModuleInterop:true}}).outputText;
  vm.runInContext(`(function(exports,require){${output}\n})`,context,{filename:file})(exports,spec=>{
    if(spec.startsWith('@tealstreet/'))return execute(path.join(root,'packages',spec.slice('@tealstreet/'.length)+'.ts'));
    if(!spec.startsWith('.'))return external(spec);
    const target=path.resolve(path.dirname(file),spec);
    return execute(fs.existsSync(target+'.ts')?target+'.ts':path.join(target,'index.ts'));
  });return exports;
}
const {DefaultStore}=execute(path.join(root,'packages/safe-cex/src/store/store.base.ts'));
const file=path.join(root,'packages/safe-cex/src/exchanges/blofin/blofin.exchange.ts');
const ast=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
const klass=ast.statements.find(s=>ts.isClassDeclaration(s)&&s.name.text==='BlofinExchange');
const methods={};
for(const name of ['symbolToMarketId','marketIdToSymbol']) {
  const member=klass.members.find(m=>m.name?.text===name);
  assert(member&&ts.isPropertyDeclaration(member)&&ts.isArrowFunction(member.initializer),'symbol_source_shape');
  const source=`(function(){ return ${member.initializer.getText(ast)}; })`;
  methods[name]=vm.runInContext(ts.transpileModule(source,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context,{filename:file});
}
const market=(id,symbol)=>({id,symbol,base:'BASE:distinct',quote:'QUOTE:distinct',active:true,precision:{amount:0.01,price:0.25},limits:{amount:{min:1,max:999},leverage:{min:2,max:17}}});
const states=[[],[market('BTC-USDT','BTCUSDT'),market('ETH-USDT','ETHUSDT')],[market('dup','first'),market('dup','second')],[market('first','dup'),market('second','dup')],[market('constructor','toString'),market('toString','constructor')],[market('雪-😀','雪😀')]];
const keys=['','BTCUSDT','ETHUSDT','BTC-USDT','ETH-USDT','missing','dup','first','second','雪-😀','雪😀',...Object.getOwnPropertyNames(Object.prototype)];
const cases=[];
function observe(value){return value===undefined?{undefined:true}:value;}
for(const markets of states) {
  const store=new DefaultStore();store.update({markets});
  for(const [name,factory] of Object.entries(methods)) {
    const fn=factory.call({store});
    for(const key of keys)cases.push({name,markets,key,expected:observe(fn(key))});
  }
}
// Explicit shared UTF-8 boundary: source preserves the unmatched lone surrogate.
const surrogate=JSON.parse('"\\ud800"');
for(const factory of Object.values(methods)) assert.equal(factory.call({store:new DefaultStore()})(surrogate).charCodeAt(0),0xd800,'source_symbol_surrogate');
// A dictionary whose prototype was replaced by a market is outside the typed Store profile.
const polluted=[market('__proto__','__proto__')];
const pollutedStore=new DefaultStore();pollutedStore.update({markets:polluted});
const boundaries=Object.entries(methods).map(([name,factory])=>({name,markets:polluted,key:'id',source:observe(factory.call({store:pollutedStore})('id'))}));
for(const boundary of boundaries) assert.deepEqual(boundary.source,{undefined:true},'source_replaced_store_prototype');
process.stdout.write(JSON.stringify({header:{architecture:process.arch,nodeMajor:Number(process.versions.node.split('.')[0]),nodeVersion:process.version},cases,boundaries}));
