const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const root=path.resolve(__dirname,'../../../packages/safe-cex/src/exchanges/woo');
const ast=ts.createSourceFile('woo.exchange.ts',fs.readFileSync(path.join(root,'woo.exchange.ts'),'utf8'),ts.ScriptTarget.Latest,true);
const cls=ast.statements.find(ts.isClassDeclaration), names=['fetchLimitOrders','fetchAlgoOrders'];
const source=names.map(name=>{const n=cls.members.find(n=>n.name?.getText(ast)===name);assert.ok(n,name);return n.getText(ast);}).join('\n');
const typeAst=ts.createSourceFile('woo.types.ts',fs.readFileSync(path.join(root,'woo.types.ts'),'utf8'),ts.ScriptTarget.Latest,true);
const endpoints=typeAst.statements.find(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>d.name.getText(typeAst)==='ENDPOINTS'));
assert.ok(endpoints,'WRAPPERS actual endpoint declaration');
const exportsContext={exports:{}};
vm.runInNewContext(ts.transpileModule(endpoints.getText(typeAst),{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,exportsContext);
function worker(text){const ctx={ENDPOINTS:exportsContext.exports.ENDPOINTS};vm.runInNewContext(ts.transpileModule(`class Witness {${text}};globalThis.Witness=Witness;`,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,ctx);return new ctx.Witness();}
function pack(v){if(v===undefined)return {$undefined:true};if(Array.isArray(v))return Array.from(v,pack);if(v&&typeof v==='object')return Object.fromEntries(Object.entries(v).map(([k,v])=>[k,pack(v)]));return v;}
const labels=['ordinary','empty','nonarray','skip-null','undefined','mapper-error','fetch-error','logger-error','append','remove','sparse','arraylike'];
async function capture(text){const cases=[];
 for(const method of names)for(const label of labels){
  const subject=worker(text),calls=[],visits=[],inputReferences=[],logs=[],mapped=[];let rows=[{id:'a'},{id:'b'},{id:'c'}];
  if(label==='empty')rows=[];if(label==='nonarray')rows={length:3};if(label==='sparse')delete rows[1];
  const thrown={message:'mapper-message',response:{data:{message:'response-message'}}};
  subject.fetchOrdersRecursively=async endpoint=>{calls.push(endpoint);if(label==='fetch-error'||label==='logger-error')throw thrown;return rows;};
  function map(row){inputReferences.push(row===undefined?"undefined":row===rows[visits.length]);const id=row?.id;visits.push(id);if(label==='mapper-error'&&id==='b')throw thrown;
   if(label==='append'&&id==='a')rows.push({id:'d'});if(label==='remove'&&id==='a')rows.pop();
   if(label==='skip-null'&&id==='b')return method==='fetchLimitOrders'?null:[];
   if(label==='undefined'&&id==='b')return undefined;
   const result={id:id??'sparse',quantity:17,price:3};mapped.push(result);
   if(method==='fetchLimitOrders')return result;
   const tail={id:result.id+'-tail'};mapped.push(tail);return label==='arraylike'?{0:result,1:tail,length:2}:[result,tail];
  }
  subject.mapLimitOrder=map;subject.mapAlgoOrder=map;
  const logger=new TypeError('reporter failure');subject.emitAndLogError=message=>{logs.push(pack(message));if(label==='logger-error')throw logger;};
  let outcome;try{const result=await subject[method]();outcome={rows:pack(result),references:Array.from(result,v=>v!==null&&typeof v==='object'?mapped.includes(v):null)};}catch(error){outcome={error:{name:error.name,message:error.message},loggerIdentity:error===logger};}
  cases.push({method,label,calls,visits:pack(visits),inputReferences,logs,outcome});
 }return cases;}
function validate(cases){assert.equal(cases.length,24,'WRAPPERS source total');let comparisons=0;
 for(let i=0;i<24;i++){const c=cases[i],algo=i>=12,label=labels[i%12];assert.equal(c.method,names[algo?1:0],'WRAPPERS method identity');assert.equal(c.label,label,'WRAPPERS label identity');assert.deepEqual(c.calls,[exportsContext.exports.ENDPOINTS[algo?'ALGO_ORDERS':'ORDERS']],'WRAPPERS route');
  let visits=['a','b','c'];if(['empty','nonarray','fetch-error','logger-error'].includes(label))visits=[];else if(label==='mapper-error'||algo&&label==='undefined')visits=['a','b'];else if(label==='append')visits=['a','b','c','d'];else if(label==='remove')visits=['a','b'];else if(label==='sparse')visits=['a',undefined,'c'];
  assert.deepEqual(c.visits,pack(visits),'WRAPPERS indexed visits '+label);
  assert.deepEqual(c.inputReferences,visits.map(id=>id===undefined?'undefined':true),'WRAPPERS input row reference '+label);
  const failure=['nonarray','mapper-error','fetch-error','logger-error'].includes(label)||algo&&label==='undefined';
  const message=label==='nonarray'?'rows.reduce is not a function':algo&&label==='undefined'?"Cannot read properties of undefined (reading 'length')":'response-message';
  assert.deepEqual(c.logs,failure?[message]:[],'WRAPPERS exact report '+label);
  if(label==='logger-error'){assert.deepEqual(c.outcome.error,{name:'TypeError',message:'reporter failure'},'WRAPPERS reporter error');assert.equal(c.outcome.loggerIdentity,true,'WRAPPERS reporter identity');continue;}
  let ids=failure?[]:visits.map(v=>v??'sparse');if(label==='skip-null')ids=ids.filter(id=>id!=='b');
  const expected=ids.flatMap(id=>algo?[{id,quantity:17,price:3},{id:id+'-tail'}]:[label==='undefined'&&id==='b'?{$undefined:true}:{id,quantity:17,price:3}]);
  assert.equal(c.outcome.rows.length,expected.length,'WRAPPERS independent output count '+label);assert.deepEqual(c.outcome.rows,expected,'WRAPPERS field identity/order '+label);assert.deepEqual(c.outcome.references,expected.map(v=>v?.$undefined?null:true),'WRAPPERS every mapper referent');comparisons++;
 }assert.equal(comparisons,22,'WRAPPERS output comparison total');}
async function main(){const cases=await capture(source);validate(cases);
 if(process.argv.includes('--controls')){
  const controls=[
   ['input-clone','this.mapLimitOrder(rows[i])','this.mapLimitOrder({...rows[i]})','WRAPPERS input row reference'],
   ['null-only','order !== null','order','WRAPPERS independent output count'],
   ['skip-row','const order = this.mapLimitOrder(rows[i]);','if(i===1)continue; const order = this.mapLimitOrder(rows[i]);','WRAPPERS indexed visits'],
   ['limit-reverse','return orders;','return orders.reverse();','WRAPPERS field identity/order'],
   ['algo-reverse','orders.push(childOrders[j]);','orders.push(childOrders[childOrders.length-1-j]);','WRAPPERS field identity/order'],
   ['clone-tail','orders.push(childOrders[j]);','orders.push({...childOrders[j]});','WRAPPERS every mapper referent'],
   ['message','err?.response?.data?.message || err?.message','err?.message || err?.response?.data?.message','WRAPPERS exact report'],
  ];
  for(const [name,from,to,assertion] of controls){
   assert.ok(source.includes(from),'WRAPPERS control admission '+name);let failure;
   try{validate(await capture(source.replace(from,to)));}catch(error){failure=error;}
   assert.ok(failure?.message.includes(assertion),'WRAPPERS named control '+name+': '+failure?.message);console.error(name+': '+assertion);
  }
 }
 process.stdout.write(JSON.stringify({capture:{architecture:process.arch,node:process.version,scope:'actual wrappers with fake fetch, mapper and reporter boundaries'},cases}));}
main().catch(error=>{console.error(error);process.exitCode=1;});
