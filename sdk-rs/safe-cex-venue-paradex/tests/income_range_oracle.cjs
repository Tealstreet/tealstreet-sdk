const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH),root=path.resolve(__dirname,'../../..');
const read=p=>fs.readFileSync(path.join(root,p),'utf8');
const source=ts.createSourceFile('exchange.ts',read('packages/safe-cex/src/exchanges/paradex/paradex.exchange.ts'),99,true);
const cls=source.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text==='ParadexExchange');
const members=cls.members.filter(n=>n.name?.getText(source)==='fetchIncomeHistoryRange');assert.equal(members.length,1,'income_range_source_identity');
const moduleObject={exports:{}};vm.runInNewContext(ts.transpileModule(`export class Harness { ${members[0].getText(source)} }`,{compilerOptions:{module:1,target:9}}).outputText,{module:moduleObject,exports:moduleObject.exports});
const {Harness}=moduleObject.exports;
const feeder=read('sdk-rs/safe-cex-venue-paradex/tests/utilities_oracle.cjs');
const numericRaw=vm.runInNewContext(feeder.slice(feeder.indexOf('const scalarRaw='),feeder.indexOf('const scalars='))+'scalarRaw').filter(raw=>typeof JSON.parse(raw)==='number');assert.equal(numericRaw.length,8,'income_range_numeric_feeder');
const base=[
 {symbol:'BTC-USD-PERP',incomeType:'FUNDING',income:11.5,asset:'USD',timestamp:1700000000123,tranId:'tx-A',tradeId:'trade-A',category:'funding'},
 {symbol:'ETH-USD-PERP',incomeType:'COMMISSION',income:-2.25,asset:'USDC',timestamp:1700000000456,tranId:'tx-B',tradeId:'trade-B',category:'commission'},
 {symbol:'SOL-USD-PERP',incomeType:'FUNDING',income:3.125,asset:'USDT',timestamp:1700000000789,tranId:'tx-C'},
 {symbol:'XRP-USD-PERP',incomeType:'',income:4.75,asset:'EUR',timestamp:1700000000912,tranId:'tx-D',category:'transfer'},
];
const kinds=[null,'','FUNDING','COMMISSION','funding','constructor','é😀','FUNDING '];
const definitions=[];
for(const [name,rows,counts] of [['mixed',base,[4,4,2,1,0,0,0,0]],['empty',[],[0,0,0,0,0,0,0,0]],['single',[base[1]],[1,1,0,1,0,0,0,0]],['duplicates',[base[0],base[0],base[1]],[3,3,2,1,0,0,0,0]]])
 kinds.forEach((kind,i)=>definitions.push({label:`${name}-${i}`,kind,rawRows:JSON.stringify(rows),startRaw:'1700000000101',endRaw:'1700000000999',expectedCount:counts[i],mode:'success'}));
for(const raw of numericRaw)for(const field of ['startRaw','endRaw'])definitions.push({label:`numeric-${field}-${raw}`,kind:'FUNDING',rawRows:JSON.stringify(base).replace('11.5',raw),startRaw:'101.25',endRaw:'209.75',[field]:raw,expectedCount:2,mode:'success'});
for(const mode of ['sync-error','async-error'])for(const kind of [null,'','FUNDING'])definitions.push({label:`${mode}-${kind}`,kind,rawRows:JSON.stringify(base),startRaw:'101.25',endRaw:'209.75',expectedCount:null,mode});
const bits=n=>{const b=Buffer.alloc(8);b.writeDoubleBE(n);return Number.isNaN(n)?'7ff8000000000000':b.toString('hex');};
function observe(v){if(typeof v==='number')return {type:'number',bits:bits(v)};if(v===undefined)return {type:'undefined'};return {type:typeof v,value:v};}
function record(r){return Object.fromEntries(['symbol','incomeType','income','asset','timestamp','tranId','tradeId','category'].map(k=>[k,observe(r[k])]));}
async function run(c){const h=new Harness(),trace=[],original={tag:'original'};let resolve,reject;const pending=new Promise((r,j)=>{resolve=r;reject=j;});const rows=JSON.parse(c.rawRows);
 h.fetchIncomeHistory=opts=>{trace.push({keys:Object.keys(opts),start:bits(opts.startTime),end:bits(opts.endTime),kind:observe(opts.incomeType)});if(c.mode==='sync-error')throw original;return pending;};
 const result=h.fetchIncomeHistoryRange(JSON.parse(c.startRaw),JSON.parse(c.endRaw),c.kind===null?undefined:c.kind).then(out=>({ok:true,sameArray:out===rows,positions:out.map(r=>rows.indexOf(r)),records:out.map(record)}),e=>({ok:false,sameError:e===original}));
 const before=trace.length;if(c.mode==='async-error')reject(original);else resolve(rows);const outcome=await result;
 return {...c,before,trace,outcome};}
(async()=>{const cases=[];for(const c of definitions)cases.push(await run(c));assert.equal(cases.length,54);assert.equal(cases.reduce((n,c)=>n+(c.outcome.records?.length||0),0),55,'income_range_source_total');
 const lone=String.fromCharCode(0xd800);const extra=await run({kind:lone,rawRows:JSON.stringify([{...base[0],incomeType:lone}]),startRaw:'1',endRaw:'2',mode:'success'});
 process.stdout.write(JSON.stringify({header:{arch:process.arch,node:process.version,nodeMajor:Number(process.versions.node.split('.')[0]),v8:process.versions.v8},cases,surrogate:{raw:JSON.stringify(lone),length:extra.outcome.records.length,codeUnit:extra.outcome.records[0].incomeType.value.charCodeAt(0)}}));})().catch(e=>{console.error(e);process.exitCode=1;});
