const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH),root=path.resolve(__dirname,'../../..');
const read=p=>fs.readFileSync(path.join(root,p),'utf8');
function execute(code,globals={}){const module={exports:{}};const out=ts.transpileModule(code,{reportDiagnostics:true,compilerOptions:{module:1,target:9}});assert.equal((out.diagnostics||[]).length,0,'orders_transpile_diagnostics');vm.runInNewContext(out.outputText,{module,exports:module.exports,...globals});return module.exports;}
const source=ts.createSourceFile('exchange.ts',read('packages/safe-cex/src/exchanges/paradex/paradex.exchange.ts'),99,true);
assert.equal(source.parseDiagnostics.length,0,'orders_source_parse');
const cls=source.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text==='ParadexExchange');
const members=cls.members.filter(n=>n.name?.getText(source)==='fetchOrders');assert.equal(members.length,1,'orders_source_identity');
const d=source.statements.filter(ts.isVariableStatement).flatMap(n=>[...n.declarationList.declarations]).find(n=>n.name.getText(source)==='describeError');
const {describeError}=execute(`export const ${d.getText(source)};`);
const {LogSeverity}=execute(read('packages/logging/src/LogSeverity.ts'));
const shared=ts.createSourceFile('types.ts',read('packages/safe-cex/src/types.ts'),99,true);
const enums=execute(shared.statements.filter(n=>ts.isEnumDeclaration(n)&&['OrderType','OrderSide'].includes(n.name.text)).map(n=>n.getText(shared)).join('\n'));
const {ENDPOINTS}=execute(read('packages/safe-cex/src/exchanges/paradex/paradex.types.ts'),{require:()=>enums});
const {Harness}=execute(`export class Harness { ${members[0].getText(source)} }`,{ENDPOINTS,LogSeverity,describeError});
const statuses=['NEW','OPEN','PENDING','UNTRIGGERED','CLOSED','CANCELED','FILLED','new','constructor','',null,13];
const base=statuses.map((status,i)=>({id:`row-${i}`,status,price:String(101.25+i*7),size:String(3.5+i*2)}));
const defs=[];const add=(label,data,extra={})=>defs.push({label,data,jwt:true,init:false,getError:'',initError:false,throwLog:-1,mapError:-1,stringifyError:false,describeError:false,...extra});
add('all-statuses',JSON.stringify({results:base}));add('reverse',JSON.stringify({results:[...base].reverse()}));add('duplicates',JSON.stringify({results:[base[1],base[1],base[0]]}));
for(const data of [null,'null','{}','false','0','""','{"results":null}','{"results":false}','{"results":0}','{"results":""}','{"results":[]}','{"results":true}','{"results":"abc"}','{"results":{}}','{"results":[null]}','{"results":[{},7,false,"x"]}'])add('shape-'+data,data);
for(const init of [false,true])for(const jwt of [false,true])add(`auth-${init}-${jwt}`,JSON.stringify({results:base}),{init,jwt});
for(const extra of [{getError:'sync'},{getError:'async'},{init:true,initError:true},{throwLog:0},{throwLog:1},{throwLog:2},{mapError:0},{mapError:2},{getError:'async',throwLog:1},{getError:'async',throwLog:2},{getError:'async',stringifyError:true},{getError:'async',describeError:true},{jwt:false,throwLog:0}])add('effect-'+JSON.stringify(extra),JSON.stringify({results:base}),extra);
async function run(c){const trace=[];let logs=0,mapCalls=0;const original={tag:'original'},replacement={tag:'replacement'},logger={tag:'logger'};
 for(const [e,tag] of [[original,'original'],[replacement,'replacement'],[logger,'logger']])Object.defineProperty(e,'toString',{value(){trace.push(['stringify',tag]);if(c.stringifyError&&tag==='original')throw replacement;return tag;}});
 Object.defineProperty(original,'toJSON',{value(){if(c.describeError)throw replacement;return {tag:'original'};}});
 const h=new Harness();let resolveInit,rejectInit,resolveGet,rejectGet;
 if(c.init)h.sdkInitPromise=new Promise((r,j)=>{resolveInit=r;rejectInit=j;});
 const pending=new Promise((r,j)=>{resolveGet=r;rejectGet=j;});
 Object.defineProperty(h,'jwtManager',{get(){trace.push(['jwt']);return c.jwt?{}:undefined;}});
 h.log=(text,severity)=>{trace.push(['log',text,severity]);if(logs++===c.throwLog)throw logger;};
 h.xhr={get:endpoint=>{trace.push(['get',endpoint]);if(c.getError==='sync')throw original;return pending;}};
 h.mapOrder=row=>{trace.push(['map',row.id]);if(mapCalls++===c.mapError)throw original;return {id:row.id,price:row.price,size:row.size};};
 const result=h.fetchOrders().then(rows=>({ok:true,rows}),e=>({ok:false,tag:e.tag||'type',name:e.name||null,message:e.message||null}));
 const before=trace.slice();if(c.init){if(c.initError)rejectInit(original);else resolveInit();await new Promise(setImmediate);}
 const initialized=trace.slice();if(c.getError==='async'&&trace.some(t=>t[0]==='get'))rejectGet(original);else resolveGet({data:c.data===null?undefined:JSON.parse(c.data)});
 const outcome=await result;return {...c,before,initialized,trace,outcome};}
(async()=>{const cases=[];for(const c of defs)cases.push(await run(c));async function boundary(data){const h=new Harness();h.jwtManager={};h.log=()=>{};h.xhr={get:async()=>({data})};h.mapOrder=r=>r.id;return h.fetchOrders();}
const inherited=await boundary(Object.create({results:[base[0]]}));let getterReads=0;const data={};Object.defineProperty(data,'results',{get(){getterReads++;return [base[1]];}});const getter=await boundary(data);
const lone=String.fromCharCode(0xd800);const surrogate=await boundary({results:[{...base[0],id:lone}]});
process.stdout.write(JSON.stringify({header:{arch:process.arch,nodeMajor:Number(process.versions.node.split('.')[0]),node:process.version},cases,boundaries:{inherited,getter,getterReads,surrogateRaw:JSON.stringify(lone),surrogateCodeUnit:surrogate[0].charCodeAt(0)}}));})().catch(e=>{console.error(e);process.exitCode=1;});
