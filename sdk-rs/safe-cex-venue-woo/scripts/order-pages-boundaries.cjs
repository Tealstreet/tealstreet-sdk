const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const file=path.join(__dirname,'order-pages-oracle.cjs');
const text=fs.readFileSync(file,'utf8');
const entry='main().catch(error=>{console.error(error);process.exitCode=1;});';
assert.equal(text.split(entry).length,2,'PAGES supplement admission');
const context={require,__dirname,process,Buffer,console};
vm.runInNewContext(text.replace(entry,'globalThis.api={worker,source,pack};'),context,{filename:file});
const {worker,source,pack}=context.api;
async function observeOrdering(text,malformed){
 const subject=worker(text),events=[];
 const metadataError=new TypeError('metadata first'),rowsError=new TypeError('rows first');
 const rows={get length(){events.push('rows:length');return 2;},
  get [Symbol.iterator](){events.push('rows:iterator');throw rowsError;}};
 const total={valueOf(){events.push('total:coerce');if(malformed)throw metadataError;return 4;}};
 const size={valueOf(){events.push('size:coerce');return 2;}};
 const meta={get total(){events.push('meta:total');return total;},
  get records_per_page(){events.push('meta:records_per_page');return size;}};
 let requests=0;
 subject.isDisposed=false;subject.emitAndLogError=()=>{throw Error('unexpected reporter');};
 subject.xhr={get:async()=>{requests++;return {data:{success:true,rows,meta}};}};
 let failure;try{await subject.fetchOrdersRecursively('/orders');}catch(error){failure=error;}
 return {events,requests,firstFailure:failure===metadataError?'metadata':failure===rowsError?'rows':'unexpected'};
}
function validateOrdering(observed,malformed){
 const expected=['rows:length','meta:total','meta:records_per_page','size:coerce','total:coerce'];
 if(!malformed)expected.push('rows:iterator');
 assert.deepEqual(observed.events,expected,'PAGES metadata first phase order');
 assert.equal(observed.firstFailure,malformed?'metadata':'rows','PAGES first failure identity');
 assert.equal(observed.requests,1,'PAGES ordering request count');
}
async function main(){
 const cases=[],ordering=[];
 for(const recursive of [false,true]){
  const subject=worker(source),first=[{id:'a'},{id:'b'}],last=[{id:'c'},{id:'d'}];let requests=0;
  subject.xhr={get:async()=>{
   requests++;
   if(requests===2)first[0].id='changed-during-next-request';
   return {data:{success:true,rows:requests===1?first:last,meta:{total:recursive?4:2,records_per_page:2}}};
  }};
  subject.emitAndLogError=()=>{throw Error('unexpected reporter');};subject.isDisposed=false;
  const result=await subject.fetchOrdersRecursively('/orders');
  assert.equal(requests,recursive?2:1,'PAGES referent request count');
  assert.equal(result.length,recursive?4:2,'PAGES referent result count');
  assert.equal(result[0],first[0],'PAGES retained element referent');
  assert.equal(result===first,!recursive,'PAGES terminal versus recursive array identity');
  const before=pack(result);first[0].id='changed-after-return';
  assert.equal(result[0].id,'changed-after-return','PAGES later element mutation');
  cases.push({recursive,requests,before,after:pack(result)});
 }
 for(const malformed of [false,true]){
  const subject=worker(source);let requests=0;
  subject.isDisposed=false;subject.emitAndLogError=()=>{throw Error('unexpected reporter');};
  subject.xhr={get:async()=>{requests++;return {data:{success:true,rows:{length:2},meta:{total:malformed?{toString:null}:4,records_per_page:2}}};}};
  let error;try{await subject.fetchOrdersRecursively('/orders');}catch(e){error={name:e.name,message:e.message};}
  assert.deepEqual(error,{name:'TypeError',message:malformed?'Cannot convert object to primitive value':'data.rows is not iterable'},'PAGES metadata before iterable failure');
  assert.equal(requests,1,'PAGES malformed request count');cases.push({malformed,requests,error});
 }
 assert.equal(cases.length,4,'PAGES supplement total');
 for(const malformed of [false,true]){
  const observed=await observeOrdering(source,malformed);validateOrdering(observed,malformed);ordering.push(observed);
 }
 assert.equal(ordering.length,2,'PAGES ordering total');
 if(process.argv.includes('--controls')){
  const from='if (data.meta.total > data.meta.records_per_page * page)';
  assert.equal(source.split(from).length,2,'PAGES ordering mutation admission');
  const mutant=source.replace(from,'[...data.rows]; '+from);
  let failure;try{validateOrdering(await observeOrdering(mutant,true),true);}catch(error){failure=error;}
  assert.ok(failure?.message.includes('PAGES metadata first phase order'),'PAGES named ordering mutation');
  console.error('early-iterable: PAGES metadata first phase order');
 }
 process.stdout.write(JSON.stringify({capture:{architecture:process.arch,node:process.version,scope:'review-time supplement, no retrospective independence'},cases,ordering}));
}
main().catch(error=>{console.error(error);process.exitCode=1;});
