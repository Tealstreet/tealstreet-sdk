const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const runtime=require('./oracle-runtime.cjs');
const root=path.resolve(__dirname,'../../../packages/safe-cex/src');
const cache=new Map();
function load(file){
 if(cache.has(file))return cache.get(file);
 let source=fs.readFileSync(file,'utf8');
 if(file===path.join(root,'types.ts')){const ast=ts.createSourceFile(file,source,ts.ScriptTarget.Latest,true);source=ast.statements.filter(ts.isEnumDeclaration).map(n=>n.getText(ast)).join('\n');}
 const exports={};cache.set(file,exports);
 vm.runInNewContext(ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,{exports,require:name=>name.startsWith('.')?load(path.resolve(path.dirname(file),name)+'.ts'):name==='@tealstreet/math/src/isNotNumber'?load(path.resolve(root,'../../math/src/isNotNumber.ts')):runtime(name)},{filename:file});
 return exports;
}
const file=path.join(root,'exchanges/woo/woo.exchange.ts');
const ast=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
const cls=ast.statements.find(ts.isClassDeclaration);
const names=['fetchHistoricalFills','fetchHistoricalTradesInternal','parseTransactionHistoryRow','normalizeRequestedSymbol','normalizeTransactionSymbol','normalizeTimestampMilliseconds','normalizeTimestampSeconds'];
const members=names.map(name=>{const m=cls.members.find(n=>n.name?.getText(ast)===name);assert.ok(m,name);return m.getText(ast)}).join('\n');
const bindings={...load(path.join(root,'exchanges/woo/woo.types.ts')),...load(path.join(root,'types.ts')),...load(path.join(root,'utils/get-key.ts')),...load(path.join(root,'utils/omitUndefined.ts')),...load(path.join(root,'exchanges/woo/woo.utils.ts'))};
function worker(source){const context={...bindings};vm.runInNewContext(ts.transpileModule('class Witness {'+source+'}\nglobalThis.Witness=Witness;',{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);return new context.Witness();}
function pack(value){
 if(value===undefined)return {$undefined:true};
 if(typeof value==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(value);return {$bits:Number.isNaN(value)?'7ff8000000000000':b.toString('hex')};}
 if(Array.isArray(value))return value.map(pack);
 if(value&&typeof value==='object')return Object.fromEntries(Object.entries(value).map(([k,v])=>[k,pack(v)]));
 return value;
}
const scenarios=['base','public','no-symbol','empty-symbol','symbol-filter','order-filter','from-fraction','to-fraction','limit-fraction','limit-zero','limit-large','limit-nan','missing-order','late-error','request-error','logger-error','fallback','fallback-error','fallback-logger-error','normalize-error','empty','signed-zero','invalid-row','prefix-symbol'];
const methods=['fetchHistoricalFills','fetchHistoricalTradesInternal'];
function inputs(label){
 const opts={symbol:'BTCUSDT',limit:500};
 const rows=['a','b','c'].map((id,i)=>({id,orderId:'order-'+id,symbol:'PERP_BTC_USDT',side:i===1?'SELL':'BUY',executedPrice:17.125+i,executedQuantity:3.75+i,executedTimestamp:1700000000000+[1,1100,1900][i],fee:0.625+i,feeAsset:'USDT',realizedPnl:-2.5-i,isMaker:i===1?1:0,positionSide:i===1?'SHORT':'LONG'}));
 if(label==='no-symbol')delete opts.symbol;
 if(label==='empty-symbol')opts.symbol='';
 if(label==='prefix-symbol')opts.symbol='WOO:BTCUSDT.P';
 if(label==='symbol-filter')rows[1].symbol='PERP_ETH_USDT';
 if(label==='order-filter')opts.orderId='order-b';
 if(label==='from-fraction')opts.from=1700000001.5;
 if(label==='to-fraction')opts.to=1700000001.5;
 if(label==='limit-fraction')opts.limit=1.9;
 if(label==='limit-zero')opts.limit=0;
 if(label==='limit-large')opts.limit=1000;
 if(label==='limit-nan')opts.limit=NaN;
 if(label==='missing-order'||label==='fallback')rows.forEach(row=>{row.orderId='';});
 if(label==='late-error'){rows[2].symbol='PERP_ETH_USDT';rows[2].executedPrice={toString:null};}
 if(label==='normalize-error')opts.symbol={};
 if(label==='signed-zero')rows.forEach(row=>{row.fee=-0;row.realizedPnl=-0;});
 if(label==='invalid-row')rows[1].executedPrice=0;
 return {opts,rows};
}
async function capture(source){
 const cases=[];
 for(const method of methods)for(const label of scenarios){
  const subject=worker(source), {opts,rows}=inputs(label), requests=[],visits=[],logs=[];
  subject.isPublicDataOnly=label==='public';
  const parser=subject.parseTransactionHistoryRow;
  subject.parseTransactionHistoryRow=row=>{visits.push(row.id);return parser(row);};
  subject.fetchTransactionHistoryRows=async query=>{
   requests.push(pack(query));
   const ordinal=requests.length;
   if(['request-error','logger-error'].includes(label)||(ordinal===2&&['fallback-error','fallback-logger-error'].includes(label)))throw {message:'outer-message',response:{data:{message:'response-message'}}};
   if(label==='empty'||(ordinal===1&&label.startsWith('fallback')))return [];
   return rows;
  };
  subject.emitAndLogError=message=>{logs.push(pack(message));if(label==='logger-error'||label==='fallback-logger-error')throw new TypeError('subscriber failure '+logs.length);};
  let outcome;
  try{const value=await subject[method](opts);outcome={rows:pack(value),ids:Array.from(value,row=>row.id)};}
  catch(error){outcome={error:{name:error.name,message:error.message}};}
  cases.push({method,label,opts:pack(opts),input:pack(rows),requests,visits,logs,outcome});
 }
 return cases;
}
function validate(cases){
 assert.equal(cases.length,48,'FILLS source total');
 let requests=0,visits=0,logs=0,outputs=0;
 for(let index=0;index<48;index++){
  const c=cases[index],fills=index<24,label=scenarios[index%24];
  assert.equal(c.method,methods[Math.floor(index/24)],'FILLS fixed method');assert.equal(c.label,label,'FILLS fixed label');
  let count=1;
  if(['public','normalize-error'].includes(label)||(!fills&&['no-symbol','empty-symbol'].includes(label)))count=0;
  else if(fills&&['fallback','fallback-error','fallback-logger-error','empty','limit-nan'].includes(label))count=2;
  assert.equal(c.requests.length,count,'FILLS request cardinality '+index);requests+=count;
  const expectedRequest={symbol:['no-symbol','empty-symbol'].includes(label)?undefined:'BTCUSDT',fromMilliseconds:label==='from-fraction'?1700000001500:undefined,toMilliseconds:label==='to-fraction'?1700000001500:undefined,limit:label==='limit-fraction'?1.9:label==='limit-zero'?1:label==='limit-nan'?NaN:500};
  for(let call=0;call<count;call++)assert.deepEqual(c.requests[call],pack(expectedRequest),'FILLS exact request fields '+index+'/'+call);
  const expectedLogs=['late-error','request-error','logger-error'].includes(label)?1: fills&&label==='fallback-error'?1:fills&&label==='fallback-logger-error'?2:0;
  assert.equal(c.logs.length,expectedLogs,'FILLS log cardinality '+index);logs+=expectedLogs;
  const expectedVisits=count===0||['request-error','logger-error','empty'].includes(label)||label==='fallback-error'||label==='fallback-logger-error'||(!fills&&label==='fallback')?0:label==='limit-nan'&&fills?6:3;
  assert.equal(c.visits.length,expectedVisits,'FILLS parse cardinality '+index);visits+=expectedVisits;
  if(expectedVisits)assert.deepEqual(c.visits,expectedVisits===6?['a','b','c','a','b','c']:['a','b','c'],'FILLS whole parse before filter');
  let ids=fills?['b','c','a']:['c','b','a'];
  if(['public','empty','limit-nan','late-error','request-error','fallback-error','fallback-logger-error','logger-error','normalize-error'].includes(label)||(!fills&&['no-symbol','empty-symbol','fallback'].includes(label)))ids=[];
  if(label==='fallback'&&fills)ids=['c','b','a'];
  if(label==='symbol-filter'||label==='invalid-row')ids=['c','a'];
  if(label==='order-filter')ids=['b'];
  if(label==='from-fraction')ids=fills?['b','c']:['c'];
  if(label==='to-fraction')ids=fills?['b','c','a']:['b','a'];
  if(label==='limit-fraction'||label==='limit-zero')ids=fills?['b']:['c'];
  const shouldThrow=label==='normalize-error'||label==='logger-error'||(fills&&label==='fallback-logger-error');
  assert.equal(Boolean(c.outcome.error),shouldThrow,'FILLS throw presence');
  if(c.outcome.error){assert.ok(['normalize-error','logger-error','fallback-logger-error'].includes(label),'FILLS expected throw');}
  else assert.deepEqual(c.outcome.ids,ids,'FILLS direct output identities '+index);
  outputs+=ids.length;
  if(!c.outcome.error)for(let outputIndex=0;outputIndex<ids.length;outputIndex++){
   const id=ids[outputIndex],i={a:0,b:1,c:2}[id],time=1700000000000+[1,1100,1900][i];
   const expected={id,symbol:'BTCUSDT',price:17.125+i,qty:3.75+i,side:i===1?'sell':'buy',commission:label==='signed-zero'?-0:0.625+i,commissionAsset:'USDT',positionSide:i===1?'short':'long',realizedPnl:label==='signed-zero'?-0:-2.5-i,maker:i===1};
   if(fills)expected.timestamp=Math.floor(time/1000);else expected.time=time;
   if(label==='fallback')expected.orderId='BTCUSDT-'+time;else if(label!=='missing-order')expected.orderId='order-'+id;else if(!fills)expected.orderId='BTCUSDT-'+time;
   assert.deepEqual(c.outcome.rows[outputIndex],pack(expected),'FILLS direct output fields '+index+'/'+outputIndex);
  }
  if(label==='late-error')assert.equal(c.logs[0],'Cannot convert object to primitive value','FILLS late coercion message');
  if(label==='request-error'||label==='fallback-error'&&fills)assert.equal(c.logs[0],'response-message','FILLS response error precedence');
  if(label==='fallback-logger-error'&&fills){assert.equal(c.logs[1],'subscriber failure 1','FILLS inner throw reaches outer catch');assert.equal(c.outcome.error.message,'subscriber failure 2','FILLS second logger throw');}
  if(label==='normalize-error'){assert.equal(c.logs.length,0,'FILLS normalization outside catch');assert.deepEqual(c.outcome.error,{name:'TypeError',message:'raw.toUpperCase is not a function'},'FILLS normalization error identity');}
  if(label==='logger-error')assert.deepEqual(c.outcome.error,{name:'TypeError',message:'subscriber failure 1'},'FILLS single logger failure');
  if(label==='fallback-logger-error'&&fills)assert.equal(c.outcome.error.name,'TypeError','FILLS double logger failure name');
  if(label==='missing-order')for(const row of c.outcome.rows){if(fills)assert.ok(!Object.hasOwn(row,'orderId'),'FILLS direct missing order omitted');else assert.equal(row.orderId,'BTCUSDT-'+(1700000000000+({a:1,b:1100,c:1900}[row.id])),'FILLS internal synthesized order');}
 }
 assert.deepEqual({requests,visits,logs,outputs},{requests:47,visits:96,logs:9,outputs:61},'FILLS fixed observation totals');
 return {requests,visits,logs,outputs};
}
async function main(){const cases=await capture(members);const totals=validate(cases);
 if(process.argv.includes('--controls'))for(const [name,before,after,count,assertion] of [
  ['limit','Math.min(Math.max(limit, 1), 500)','500',2,'FILLS exact request fields'],
  ['error-message','err?.response?.data?.message || err?.message','err?.message',2,'FILLS response error precedence'],
  ['fallback','if (normalizedRequestedSymbol && executions.length === 0) {','if (false) {',1,'FILLS request cardinality'],
  ['seconds-sort','b.timestamp - a.timestamp','a.timestamp - b.timestamp',2,'FILLS direct output identities'],
  ['parse-before-filter','this.parseTransactionHistoryRow(row)','row.symbol === "PERP_ETH_USDT" ? null : this.parseTransactionHistoryRow(row)',2,'FILLS parse cardinality'],
  ['field-identity','commission: entry!.commission','commission: entry!.qty',2,'FILLS direct output fields'],
 ]){
  assert.equal(members.split(before).length,count+1,'FILLS mutation admission '+name);
  const mutated=await capture(members.replace(before,after));
  assert.throws(()=>validate(mutated),e=>e.code==='ERR_ASSERTION'&&e.message.includes(assertion),'FILLS source control '+name);
  console.error('FILLS source control assertion-red: '+name);
 }
 process.stdout.write(JSON.stringify({architecture:process.arch,nodeMajor:process.versions.node.split('.')[0],totals,cases}));}
main().catch(error=>{console.error(error);process.exitCode=1;});
