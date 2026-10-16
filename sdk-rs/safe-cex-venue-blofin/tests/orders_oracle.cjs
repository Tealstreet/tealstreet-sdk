// Execute the actual mapOrders initializer and imported source helpers.
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
const member=klass.members.find(m=>m.name?.text==='mapOrders');
assert(member&&ts.isPropertyDeclaration(member)&&ts.isArrowFunction(member.initializer),'orders_source_shape');
const selected=new Map();
for(const statement of ast.statements)if(ts.isVariableStatement(statement))for(const d of statement.declarationList.declarations)if(['BLOFIN_SUPPORTED_INST_TYPE','getNormalizedInstType','isKnownNonSwapInstrument'].includes(d.name.text))selected.set(d.name.text,d.initializer);
assert.equal(selected.size,3,'orders_helper_census');
let helpers='';for(const name of ['BLOFIN_SUPPORTED_INST_TYPE','getNormalizedInstType','isKnownNonSwapInstrument'])helpers+=`const ${name} = ${selected.get(name).getText(ast)};\n`;
Object.assign(context,execute(path.join(root,'packages/safe-cex/src/exchanges/blofin/blofin.types.ts')));
Object.assign(context,execute(path.join(root,'packages/safe-cex/src/types.ts')));
Object.assign(context,execute(path.join(root,'packages/safe-cex/src/utils/safe-math.ts')));
const clock=[];context.virtualClock={getCurrentTimeMs(){const value=1700000000000.125+clock.length*0.375;clock.push(value);return value;}};
let initializer=member.initializer.getText(ast);
if(process.env.SAFE_CEX_BLOFIN_AUDIT_SOURCE_CLOCK==='1') {
 const statement='virtualClock.getCurrentTimeMs()';
 assert(initializer.includes(statement),'BLOFIN-AUDIT source control selector');
 initializer=initializer.replaceAll(statement,'(virtualClock.getCurrentTimeMs(), virtualClock.getCurrentTimeMs())');
}
const factory=vm.runInContext(ts.transpileModule(`(function(){${helpers}return ${initializer};})`,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context,{filename:file});
function observe(value){
 if(value===undefined)return{undefined:true};
 if(typeof value==='function')return{function:value.name};
 if(value===Object.prototype)return{prototype:true};
 if(typeof value==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(value);return{number:Number.isNaN(value)?'nan':b.toString('hex')};}
 if(Array.isArray(value))return Array.from(value,observe);
 if(value&&typeof value==='object')return Object.fromEntries(Object.entries(value).map(([k,v])=>[k,observe(v)]));return value;
}
const market={id:'BTC-USDT',symbol:'BTCUSDT',base:'BTC',quote:'USDT',active:true,contractSize:0.125,info:{contractValue:'0.375'},precision:{amount:0.01,price:0.25},limits:{amount:{min:1,max:999},leverage:{min:2,max:17}}};
const base={instId:'BTC-USDT',instType:'SWAP',orderId:'order-one',state:'live',orderCategory:'normal',orderType:'limit',side:'buy',size:'7.75',filledSize:'2.125',price:'19.875',reduceOnly:'false'};
const cases=[];
function run(raw,markets=[market],inheritedMarket=false){
 const store=new DefaultStore();store.update({markets});const fn=factory.call({store});clock.length=0;
 let result,invalid=false,refusalPrefix=null,aliases=[];try{const input=JSON.parse(raw);const orders=fn(input);const invalidIndex=orders.findIndex(o=>typeof o.id!=='string'||typeof o.symbol!=='string'||!Object.values(context.OrderSide).includes(o.side)||!Object.values(context.OrderStatus).includes(o.status)||!Object.values(context.OrderType).includes(o.type));invalid=invalidIndex!==-1;refusalPrefix=invalid?invalidIndex:null;for(let a=0;a<orders.length;a++){assert(input.includes(orders[a].info),"source_orders_info_alias");for(let b=a+1;b<orders.length;b++)if(orders[a].info===orders[b].info)aliases.push([a,b]);}result={ok:observe(orders)};}catch(error){result={error:error.name,message:error.message};}
 cases.push({raw,markets,result,clock:observe(clock),invalid,refusalPrefix,aliases,inheritedMarket});
}
run('[]');run(JSON.stringify([base]));run(JSON.stringify([base,{...base,side:'unknown'},base]));
const seeds=[base,{...base,orderType:'trigger',algoId:'opening-id',triggerPrice:'11.25',orderCategory:'algo'}, {...base,tpslId:'protect-id',tpTriggerPrice:'29.5',slTriggerPrice:'13.75'}];
const values=['null','true','false','0','-0','1e400','-1e-400','9007199254740993','[]','["2.75tail"]','{}','{"toString":null}',...['',' ','-0','2.5tail','Infinity','NaN','constructor','__proto__','buy','sell','live','filled','SWAP','spot','true','trigger','normal'].map(JSON.stringify)];
const fields=['instId','instType','orderId','state','orderCategory','orderType','side','size','filledSize','price','reduceOnly','tpslId','algoId','triggerPrice','tpTriggerPrice','slTriggerPrice'];
for(const seed of seeds){run(JSON.stringify([seed]));for(const key of fields){const absent={...seed};delete absent[key];run(JSON.stringify([absent]));for(const value of values){const pairs=Object.entries({...seed,[key]:null}).map(([k,v])=>JSON.stringify(k)+':'+(k===key?value:JSON.stringify(v)));run('[{'+pairs.join(',')+'}]');}}}
for(const orderType of ['market','limit','post_only','fok','ioc','optimal_limit_ioc'])for(const side of ['buy','sell'])for(const state of ['live','partially_filled'])run(JSON.stringify([{...base,orderType,side,state}]));
for(const state of ['canceled','order_failed','effective','live','unknown'])for(const reduceOnly of [true,false,'true','false'])run(JSON.stringify([{...seeds[1],state,reduceOnly}]));
for(const contractSize of [0,-0,0.375,-2.5])for(const contractValue of ['0','NaN','Infinity','-0.125','2.75tail',{},null])run(JSON.stringify(seeds),[{...market,contractSize,info:{contractValue}}]);
for(const info of [null,{},'text'])run(JSON.stringify([base]),[{...market,contractSize:0,info}]);
const noInfo={...market,contractSize:0};delete noInfo.info;run(JSON.stringify([base]),[noInfo]);
for(const key of Object.getOwnPropertyNames(Object.prototype))run(JSON.stringify([{...base,instId:key}]),[market],true);
for(const raw of ['null','{}','true','"text"','[null]',JSON.stringify([base,null]),JSON.stringify([{...base,instId:'missing'},...seeds]),JSON.stringify(seeds)])run(raw);
run('[{"instId":"BTC-USDT","orderId":"retained","state":"live","orderCategory":"normal","orderType":"limit","side":"buy","price":1e400,"size":9007199254740993,"filledSize":-0,"opaque":-1e-400}]');
const boundaries=['[{"instId":"\\ud800"}]','['.repeat(140)+'0'+']'.repeat(140)];
for(const raw of boundaries){const store=new DefaultStore();assert.doesNotThrow(()=>factory.call({store})(JSON.parse(raw)),'source_orders_boundary');}
assert(cases.some(c=>c.invalid),'orders_typed_boundary_covered');
const audit=[];
for(const rows of [[{...base,side:'constructor'},null],[{...base,side:'constructor',price:{toString:null}}]]) {
 run(JSON.stringify(rows));audit.push(cases.pop());
}
assert.equal(audit.length,2,'BLOFIN-AUDIT source count');
process.stdout.write(JSON.stringify({header:{architecture:process.arch,nodeMajor:Number(process.versions.node.split('.')[0]),nodeVersion:process.version},cases,boundaries,audit}));
