const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH),root=path.resolve(__dirname,'../../..');
const read=p=>fs.readFileSync(path.join(root,p),'utf8');
function execute(code,globals={}){const module={exports:{}};vm.runInNewContext(ts.transpileModule(code,{compilerOptions:{module:1,target:9}}).outputText,{module,exports:module.exports,...globals});return module.exports;}
const source=ts.createSourceFile('exchange.ts',read('packages/safe-cex/src/exchanges/paradex/paradex.exchange.ts'),99,true);
const cls=source.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text==='ParadexExchange');
const members=cls.members.filter(n=>n.name?.getText(source)==='fetchBalance');assert.equal(members.length,1);
const d=source.statements.filter(ts.isVariableStatement).flatMap(n=>[...n.declarationList.declarations]).find(n=>n.name.getText(source)==='describeError');
const {describeError}=execute(`export const ${d.getText(source)};`);
const {LogSeverity}=execute(read('packages/logging/src/LogSeverity.ts'));
const shared=ts.createSourceFile('types.ts',read('packages/safe-cex/src/types.ts'),99,true);
const enums=execute(shared.statements.filter(n=>ts.isEnumDeclaration(n)&&['OrderType','OrderSide'].includes(n.name.text)).map(n=>n.getText(shared)).join('\n'));
const {ENDPOINTS}=execute(read('packages/safe-cex/src/exchanges/paradex/paradex.types.ts'),{require:()=>enums});
let trace=[];
const {Harness}=execute(`export class Harness { ${members[0].getText(source)} }`,{ENDPOINTS,LogSeverity,describeError:e=>{trace.push(['describe',e.tag||'native']);return describeError(e);}});
// Reuse the existing numeric/coercion feeder bytes, without editing it or spawning it.
const feeder=read('sdk-rs/safe-cex-venue-paradex/tests/utilities_oracle.cjs');
const scalarRaw=vm.runInNewContext(feeder.slice(feeder.indexOf('const scalarRaw='),feeder.indexOf('const scalars='))+'scalarRaw');
assert.equal(scalarRaw.length,39,'balance_shared_feeder_census');
const fields=['total_collateral','account_value','equity','margin_balance','wallet_balance','free_collateral','free','available_margin','available_collateral','initial_margin_requirement','position_margin','used_margin'];
const base=Object.fromEntries(fields.map((key,i)=>[key,String(101.25+i*17.5)]));
const defs=[];const add=(label,raw,extra={})=>defs.push({label,raw,jwt:true,init:false,...extra});
add('distinct-fields',JSON.stringify(base));add('wrapped-distinct',JSON.stringify({data:base}));
for(const field of fields)for(const raw of scalarRaw)add(`numeric-${field}-${raw}`,JSON.stringify({...base,[field]:'__RAW__'}).replace('"__RAW__"',raw));
for(const group of [fields.slice(0,5),fields.slice(5,9),fields.slice(9)])for(let i=0;i<group.length;i++){const value={...base};for(const k of group.slice(0,i))delete value[k];add(`precedence-${group[i]}`,JSON.stringify(value));}
for(const raw of ['{}','[]','true','false','null','0','-0','""','"x"','{"data":null}','{"data":false}','{"data":0}','{"data":{}}'])add('shape-'+raw,raw);
add('undefined',null);add('clamped-used','{"total_collateral":"5","free_collateral":"9"}');add('distinct-derived-used','{"total_collateral":"107.75","free_collateral":"13.25"}');add('overflow-used','{"total_collateral":"1e308","free_collateral":"-1e308"}');
for(const init of [false,true])for(const jwt of [false,true])add(`auth-${init}-${jwt}`,JSON.stringify(base),{init,jwt});
for(const extra of [{getError:'sync'},{getError:'async'},{init:true,initError:true},{throwLog:0},{throwLog:1},{getError:'async',throwLog:1},{getError:'async',throwLog:2},{getError:'async',stringifyError:true},{getError:'async',describeError:true},{getError:'async',describeUndefined:true},{jwt:false,throwLog:0}])add('effect-'+JSON.stringify(extra),JSON.stringify(base),extra);
const bits=n=>{const b=Buffer.alloc(8);b.writeDoubleBE(n);return Number.isNaN(n)?'7ff8000000000000':b.toString('hex');};
async function run(c){trace=[];let initDone=!c.init,resolveInit,rejectInit,resolveGet,rejectGet;let logs=0,stringCalls=0;
 const initPromise=new Promise((r,j)=>{resolveInit=r;rejectInit=j;});const getPromise=new Promise((r,j)=>{resolveGet=r;rejectGet=j;});
 const replacement={tag:'replacement',message:'replacement'};
 const original={tag:'original',message:'network',toString(){trace.push(['stringify','original']);stringCalls++;if(c.stringifyError||(c.describeError&&stringCalls===2))throw replacement;return 'original:network';},toJSON(){if(c.describeError)throw replacement;if(c.describeUndefined)return undefined;return {tag:'original',message:'network'};}};
 const logger={tag:'logger',message:'logger',toString(){trace.push(['stringify','logger']);return 'logger:logger';}};
 const h=new Harness();h.sdkInitPromise=c.init?initPromise:undefined;
 Object.defineProperty(h,'jwtManager',{get(){trace.push(['jwt']);return c.jwt&&initDone?{}:undefined;}});
 h.log=(message,severity)=>{trace.push(['log',message,severity]);if(logs++===c.throwLog)throw logger;};
 h.xhr={get(url){trace.push(['get',url]);if(c.getError==='sync')throw original;return getPromise;}};
 const result=h.fetchBalance().then(b=>({ok:true,keys:Object.keys(b),balance:Object.fromEntries(['total','free','used','upnl'].map(k=>[k,bits(b[k])]))}),e=>({ok:false,tag:e===original?'original':e===logger?'logger':e===replacement?'replacement':'native',name:e.name||null,message:e.message||null}));
 const before=trace.map(x=>[...x]);initDone=true;
 if(c.init){if(c.initError)rejectInit(original);else resolveInit();}
 // Drain cross-realm promise adoption without settling the held GET promise.
 await new Promise(resolve=>setImmediate(resolve));
 const afterInit=trace.map(x=>[...x]);
 if(c.getError==='async'&&!c.initError&&c.jwt&&c.throwLog!==0)rejectGet(original);else resolveGet({data:c.raw===null?undefined:JSON.parse(c.raw)});
 const outcome=await result;return {...c,before,afterInit,trace:trace.map(x=>[...x]),outcome};
}
(async()=>{const cases=[];for(const c of defs)cases.push(await run(c));
// Witness the boundary instead of silently treating getters as plain JSON data.
const getterTrace=[];const exotic={get total_collateral(){getterTrace.push('total');return '7';},get account_value(){getterTrace.push('account');throw Error('getter');}};
const h=new Harness();h.jwtManager={};h.xhr={get:async()=>({data:exotic})};h.log=()=>{};let getterMessage;try{await h.fetchBalance();}catch(e){getterMessage=e.message;}
const lone=String.fromCharCode(0xd800);const surrogateRaw=JSON.stringify({total_collateral:lone});let surrogateLogCodeUnit;
h.xhr={get:async()=>({data:JSON.parse(surrogateRaw)})};h.log=message=>{const i=message.indexOf(lone);if(i>=0)surrogateLogCodeUnit=message.charCodeAt(i);};await h.fetchBalance();
process.stdout.write(JSON.stringify({surrogateRaw,surrogateLogCodeUnit,header:{arch:process.arch,node:process.version,nodeMajor:Number(process.versions.node.split('.')[0]),v8:process.versions.v8},cases,getterTrace,getterMessage}));})().catch(e=>{console.error(e);process.exitCode=1;});
