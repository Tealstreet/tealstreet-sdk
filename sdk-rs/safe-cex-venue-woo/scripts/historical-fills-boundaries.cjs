const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const file=path.join(__dirname,'historical-fills-oracle.cjs');
const source=fs.readFileSync(file,'utf8');
const entry='main().catch(error=>{console.error(error);process.exitCode=1;});';
assert.equal(source.split(entry).length,2,'FILLS boundary definition admission');
const context={require,__dirname,process,Buffer,console};
vm.runInNewContext(source.replace(entry,'')+'\nglobalThis.fixture={worker,members,inputs,pack};',context,{filename:file});
const {worker,members,inputs,pack}=context.fixture;
const plain=value=>JSON.parse(JSON.stringify(pack(value)));
async function main(){
 const cases=[];
 for(const method of ['fetchHistoricalFills','fetchHistoricalTradesInternal'])for(const label of ['discarded-asset','retained-asset','later-coercion','from-nan','from-positive','from-negative','to-nan','to-positive','to-negative']){
  const subject=worker(members),{opts,rows}=inputs('base'),requests=[],logs=[];
  if(label.endsWith('asset')||label==='later-coercion')rows[0].feeAsset={label:'unrepresentable'};
  if(label==='discarded-asset')opts.limit=1;
  if(label==='later-coercion')rows[2].executedPrice={toString:null};
  if(label.startsWith('from-')||label.startsWith('to-'))opts[label.split('-')[0]]=label.endsWith('nan')?NaN:label.endsWith('positive')?Infinity:-Infinity;
  subject.isPublicDataOnly=false;
  subject.fetchTransactionHistoryRows=async query=>{requests.push(plain(query));return rows;};
  subject.emitAndLogError=message=>logs.push(message);
  const result=await subject[method](opts);
  assert.equal(requests.length,1,'FILLS boundary single fetch');
  assert.equal(logs.length,label==='later-coercion'?1:0,'FILLS boundary report count');
  assert.equal(result.length,label==='later-coercion'?0:label==='discarded-asset'?1:3,'FILLS boundary source row count');
  if(label==='later-coercion')assert.equal(logs[0],'Cannot convert object to primitive value','FILLS boundary later parser error');
  if(label==='retained-asset')assert.equal(result.find(row=>row.id==='a').commissionAsset,rows[0].feeAsset,'FILLS boundary source asset alias');
  if(label.startsWith('from-')||label.startsWith('to-'))assert.equal(requests[0][label.startsWith('from-')?'fromMilliseconds':'toMilliseconds'].$undefined,true,'FILLS nonfinite bound omitted');
  cases.push({method,label,opts:plain(opts),input:plain(rows),requests,logs,rows:plain(result)});
 }
 assert.equal(cases.length,18,'FILLS boundary total');
 assert.equal(cases.reduce((sum,c)=>sum+c.rows.length,0),44,'FILLS boundary total outputs');
 process.stdout.write(JSON.stringify({architecture:process.arch,nodeMajor:process.versions.node.split('.')[0],cases}));
}
main().catch(error=>{console.error(error);process.exitCode=1;});
