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
const member=klass.members.find(m=>m.name?.text==='formatTickers');
assert(member&&ts.isPropertyDeclaration(member)&&ts.isArrowFunction(member.initializer),'ticker_source_shape');
const roundUSD=execute(path.join(root,'packages/safe-cex/src/utils/roundUSD.ts')).roundUSD;
context.roundUSD=roundUSD;
const factory=vm.runInContext(ts.transpileModule(`(function(){return ${member.initializer.getText(ast)};})`,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context,{filename:file});
function observe(value){
  if(value===undefined)return{undefined:true};
  if(typeof value==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(Number.isNaN(value)?NaN:value);return{number:Number.isNaN(value)?'nan':b.toString('hex')};}
  if(Array.isArray(value))return Array.from(value,observe);
  if(value&&typeof value==='object')return Object.fromEntries(Object.entries(value).map(([k,v])=>[k,observe(v)]));
  return value;
}
const markets=[{id:'BTC-USDT',symbol:'BTCUSDT',base:'BTC',quote:'USDT',active:true,precision:{amount:0.01,price:0.25},limits:{amount:{min:1,max:999},leverage:{min:2,max:17}}}];
const fallback={id:'stored-id',symbol:'stored-symbol',bid:1.125,ask:2.25,last:3.375,mark:4.5,index:5.625,percentage:6.75,fundingRate:7.875,fundingTime:8.5,volume:9.125,quoteVolume:10.25,openInterest:11.375};
const base={instId:'BTC-USDT',open24h:'12.25',last:'13.75',bidPrice:'13.5',askPrice:'14.125',volCurrency24h:'17.375',vol24h:'29.625'};
const cases=[];
function run(raw,hostThrows=false,inherited=false,hostMutates=false,multiFallback=false){
 const store=new DefaultStore();const fallbackRows=multiFallback?[fallback,{...fallback,id:"second-stored",symbol:"second-symbol",last:42.875}]:[fallback];store.update({markets,tickers:fallbackRows});
 const sentinel=new Error('host:unique');
 const logs=[];const fn=factory.call({store,emitAndLogError(message){logs.push(message);if(hostMutates)store.update({tickers:[{...fallback,id:'after-log',symbol:'after-log-symbol',last:37.125}]});if(hostThrows)throw sentinel;}});
 let result,returned;try{returned=fn(JSON.parse(raw));result={ok:observe(returned)}}catch(error){result={error:error.name,message:error.message,identity:error===sentinel};}
 cases.push({...multiFallback?{fallbackRows}: {},raw,hostThrows,inherited,hostMutates,logs,result,fallbackAlias:returned===store.tickers,finalTickers:observe(store.tickers)});
}
const values=['null','true','false','0','-0','-1e-400','1e400','-1e400','9007199254740993','[]','["2.75tail"]','{}','{"toString":null}'];
for(const value of ['',' ','-0','0','-2.5','12.75tail','NaN','Infinity','0x10','1e2','\ufeff4','\u00854','雪😀'])values.push(JSON.stringify(value));
run(JSON.stringify([base]));run('[]');
for(const key of Object.keys(base)){
 const absent={...base};delete absent[key];run(JSON.stringify([absent]));
 for(const value of values){const pairs=Object.entries(base).map(([k,v])=>JSON.stringify(k)+':'+(k===key?value:JSON.stringify(v)));run('[{'+pairs.join(',')+'}]');}
}
for(const raw of ['null','true','{}','"text"','[null]',JSON.stringify([base,null]),JSON.stringify([{...base,instId:'missing'},base])]){run(raw);run(raw,true);}
for(const key of Object.getOwnPropertyNames(Object.prototype))run(JSON.stringify([{...base,instId:key}]),false,true);
run('[{"instId":"BTC-USDT","open24h":-0,"last":-0,"bidPrice":-0,"askPrice":1e400,"volCurrency24h":9007199254740993,"vol24h":-1e-400}]');
run('[null]',false,false,true);run('[null]',true,false,true);
const originalCases = Buffer.from(JSON.stringify(cases));
assert.equal(originalCases.length, 285919, 'BLOFIN-CARDINALITY original corpus bytes');
assert.equal(require('node:crypto').createHash('sha256').update(originalCases).digest('hex'), '69191a07baa06e086d19852283624cbc5da9b71f318279c3f23c6febea457669', 'BLOFIN-CARDINALITY original corpus identity');
// Append multi-output witnesses; baseline 220 cases are byte-identical.
run(JSON.stringify([base,{...base,last:'27.875',bidPrice:'26.125',askPrice:'28.375'}]));
run('[null]',false,false,false,true);
const boundaries=['[{"instId":"\\ud800"}]','['.repeat(140)+'0'+']'.repeat(140)];
for(const raw of boundaries){const store=new DefaultStore();const fn=factory.call({store,emitAndLogError(){}});assert.doesNotThrow(()=>fn(JSON.parse(raw)),'source_ticker_boundary');}
process.stdout.write(JSON.stringify({header:{architecture:process.arch,nodeMajor:Number(process.versions.node.split('.')[0]),nodeVersion:process.version},markets,fallback:observe(fallback),fallbackRaw:fallback,cases,boundaries}));
