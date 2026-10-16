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
const getKey=execute('packages/safe-cex/src/utils/get-key.ts', Object.fromEntries(['camelCase','snakeCase','memoize'].map(name=>['lodash/'+name,require('lodash/'+name)])));
function capture(value) {
 if(value===undefined)return {kind:'undefined'};
 if(value===null)return {kind:'null'};
 if(typeof value==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(value);return {kind:'number',bits:Number.isNaN(value)?'NaN':b.toString('hex')};}
 if(Array.isArray(value))return {kind:'array',items:value.map(capture)};
 if(typeof value==='object')return {kind:'object',entries:Object.entries(value).map(([key,v])=>[key,capture(v)])};
 return {kind:typeof value,value};
}
const names=['formatSymbolIn','parseTickers','fetchTickersRaw','fetchTickers'];
const bodies=Object.fromEntries(names.map(name=>{
 const members=owner[0].members.filter(m=>m.name?.getText(ast)===name);
 assert.equal(members.length,1,'actual '+name+' body');
 return [name,ts.transpileModule(`const method=${members[0].initializer.getText(ast)};`,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText];
}));
const market=(symbol,id)=>({id,symbol,base:'base',quote:'quote',active:true,precision:{amount:1,price:0.1},limits:{amount:{min:0,max:999},leverage:{min:1,max:20}}});
const markets=[market('BTCUSDT','btc-id'),market('ETHUSDC','eth-id'),market('XUSD','x-id'),market('','empty-id')];
const cached=[0,1].map(i=>({id:'cached-'+i,symbol:'CACHE'+i,bid:101+i,ask:103+i,last:107+i,mark:109+i,index:113+i,percentage:127+i,openInterest:131+i,fundingRate:137+i,fundingTime:139+i,volume:149+i,quoteVolume:151+i}));
const row=(symbol='BTCUSDT')=>({symbol,bidRp:'11.25',askRp:'12.5',lastRp:'17.75',markRp:'19.125',indexRp:'23.5',openRp:'13.25',openInterestRv:'29.75',fundingRateRr:'0.0031',volumeRq:'31.5'});
const cases=[];
const add=(label,data,count)=>cases.push({label,raw:JSON.stringify(data)??null,count});
for(const data of [undefined,null,false,0,'',{},[],{result:null},{result:0},{result:false},{result:''}])add('fallback-'+cases.length,data,2);
add('empty',{result:[]},0);
add('both settlements',{result:[row(),row('ETHUSDC')]},2);
add('filtered middle and tail',{result:[row(),row('UNKNOWNUSDT'),row('btcusdt'),row('XUSD'),row('ETHUSDC'),row('OTHERUSDC')]},2);
for(const value of [true,1,'bad',{}])add('invalid result-'+cases.length,{result:value},null);
for(const value of [null,{},23,false,[],{symbol:null},{symbol:1},{symbol:false},{symbol:[]},{symbol:{}}])add('invalid row-'+cases.length,{result:[value]},null);
for(const [field,value] of [['bidRp','-0'],['askRp','1e309'],['lastRp','nonsense'],['markRp',[7,9]],['indexRp',null],['openRp','0'],['openRp','-0'],['openInterestRv',false],['fundingRateRr','-Infinity'],['volumeRq','12.25tail'],['bidRp',{}]])add('numeric-'+field+'-'+cases.length,{result:[{...row(),[field]:value},row('ETHUSDC')]},2);
add('coercion failure',{result:[{...row(),bidRp:{toString:0}}]},null);
// Raw lexemes retain nonfinite and >2^53 Number ingress.
cases.push({label:'raw numeric lexemes',raw:'{"result":[{"symbol":"BTCUSDT","bidRp":-0,"askRp":1e400,"lastRp":9007199254740993,"markRp":5e-324,"indexRp":-1e400,"openRp":3,"openInterestRv":7,"fundingRateRr":11,"volumeRq":13}]}',count:1});
(async()=>{
 for(const fixture of cases){
  const context={store:{marketsBySymbol:Object.fromEntries(markets.map(m=>[m.symbol,m])),tickers:cached}};
  for(const name of ['formatSymbolIn','parseTickers'])context[name]=vm.runInNewContext('{'+bodies[name]+'\nmethod;}',context);
  try{fixture.expected={ok:true,value:capture(context.parseTickers(fixture.raw===null?undefined:JSON.parse(fixture.raw)))}}catch(error){fixture.expected={ok:false,name:error.name,message:error.message}};
 }
 const reads=[];
 for(const fixture of [
  {label:'read success',raw:JSON.stringify({error:null,result:[row(),row('ETHUSDC')]}),count:2},
  {label:'read fallback',raw:'{"error":"denied"}',count:2},
  {label:'missing error',raw:'{}',count:2},
  {label:'read parser failure',raw:'{"error":null,"result":true}',count:null},
  {label:'read rejection',readFailure:{name:'ReadError',message:'read stopped'},count:null},
  {label:'emit rejection',raw:'{"error":"denied"}',emitFailure:{name:'EmitError',message:'emit stopped'},count:null}
 ]){
  const events=[];
  const context={ENDPOINTS:constants.ENDPOINTS,v:getKey.v,store:{marketsBySymbol:Object.fromEntries(markets.map(m=>[m.symbol,m])),tickers:cached},xhr:{get:async url=>{events.push(['read',url]);if(fixture.readFailure)throw fixture.readFailure;return {data:JSON.parse(fixture.raw)}}},emitAndLogError:value=>{events.push(['emit',capture(value)]);if(fixture.emitFailure)throw fixture.emitFailure;}};
  for(const name of names)context[name]=vm.runInNewContext('{'+bodies[name]+'\nmethod;}',context);
  try{fixture.expected={ok:true,value:capture(await context.fetchTickers())}}catch(error){fixture.expected={ok:false,name:error.name,message:error.message,readIdentity:error===fixture.readFailure,emitIdentity:error===fixture.emitFailure}};
  fixture.events=events;reads.push(fixture);
 }
 process.stdout.write(JSON.stringify({profile:{node:process.version,major:process.versions.node.split('.')[0],arch:process.arch,platform:process.platform},markets,cached,cases,reads}));
})().catch(error=>{console.error(error);process.exitCode=1});
