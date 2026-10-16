const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
assert.ok(process.env.SAFE_CEX_TYPESCRIPT_PATH,'SAFE_CEX_TYPESCRIPT_PATH required');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH),root=path.resolve(__dirname,'../../..');
const read=p=>fs.readFileSync(path.join(root,p),'utf8');
const header={arch:process.arch,node:process.version,nodeMajor:Number(process.versions.node.split('.')[0]),v8:process.versions.v8};
function execute(code,imports={}){const module={exports:{}};vm.runInNewContext(ts.transpileModule(code,{compilerOptions:{module:1,target:9}}).outputText,{module,exports:module.exports,...imports});return module.exports;}
const shared=ts.createSourceFile('shared.ts',read('packages/safe-cex/src/types.ts'),99,true);
const enums=shared.statements.filter(n=>ts.isEnumDeclaration(n)&&['PositionMode','OrderType','OrderSide'].includes(n.name.text));
assert.equal(enums.length,3,'shared_enum_census');
const bindings=execute(enums.map(n=>n.getText(shared)).join('\n'));
const {LogSeverity}=execute(read('packages/logging/src/LogSeverity.ts'));
const venue=execute(read('packages/safe-cex/src/exchanges/paradex/paradex.types.ts'),{require:name=>{assert.equal(name,'../../types');return bindings;}});
const source=ts.createSourceFile('exchange.ts',read('packages/safe-cex/src/exchanges/paradex/paradex.exchange.ts'),99,true);
const cls=source.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text==='ParadexExchange');
const members=cls.members.filter(n=>['getAccount','getPositionMode'].includes(n.name?.getText(source)));
assert.equal(members.length,2,'account_source_census');
const {Harness}=execute(`export class Harness { ${members.map(n=>n.getText(source)).join('\n')} }`,{ENDPOINTS:venue.ENDPOINTS,LogSeverity,PositionMode:bindings.PositionMode});
const base={account:'account-distinct',account_number:'user-001',address:'address-distinct',public_key:'public-distinct',initial_margin_requirement:'1.1',maintenance_margin_requirement:'2.2',account_value:'3.3',total_collateral:'4.4',free_collateral:'5.5',margin_cushion:'6.6',settlement_asset:'USDC',updated_at:1700000000123,status:'ACTIVE',seq_no:17};
const definitions=[];
for(const id of ['user-001','','0','000001','用户😀','constructor','__proto__']) definitions.push({label:`id-${id}`,mode:'success',data:{...base,account_number:id}});
for(const mode of ['sync-error','async-error','coercion-error','logger-error'])for(const message of ['network-down','','é😀'])definitions.push({label:`${mode}-${message}`,mode,message});
async function run(c){
 const trace=[];const h=new Harness();
 let settle;const pending=new Promise((resolve,reject)=>{settle={resolve,reject};});
 const original={tag:'original',message:c.message,toString(){trace.push(['stringify']);if(c.mode==='coercion-error')throw coercion;return c.message;}};
 const coercion={tag:'coercion'},logging={tag:'logger'};
 h.xhr={get(url){trace.push(['get',url]);if(c.mode==='sync-error')throw original;return pending;}};
 h.log=(message,severity)=>{trace.push(['log',message,severity]);if(c.mode==='logger-error')throw logging;};
 const result=h.getAccount();
 // Attach handling before resolving/rejecting the host promise.
 const captured=result.then(value=>({ok:true,userId:value.userId,affiliateId:value.affiliateId,keys:Object.keys(value)}),error=>({ok:false,identity:error===original?'original':error===coercion?'coercion':error===logging?'logger':'other'}));
 const before=trace.map(x=>[...x]);
 if(c.mode==='success')settle.resolve({data:c.data});else if(c.mode!=='sync-error')settle.reject(original);
 const outcome=await captured;
 return {...c,before,trace,outcome};
}
(async()=>{
 const cases=[];for(const c of definitions)cases.push(await run(c));
 const h=new Harness();h.xhr={get(){throw Error('position mode must not request');}};h.log=()=>{throw Error('position mode must not log');};
 const positionMode=await h.getPositionMode();
 // Source accepts an escaped lone surrogate while Rust String refuses it.
 const lone=JSON.parse('"\\ud800"');const surrogate=await run({label:'surrogate',mode:'success',data:{...base,account_number:lone}});
 // The host's declared AccountResponse ingress is stricter than unchecked JS.
 const unchecked=await run({label:'missing-id',mode:'success',data:{...base,account_number:undefined}});
 const uncheckedType=typeof unchecked.outcome.userId;
 process.stdout.write(JSON.stringify({header,cases,positionMode,surrogateCodeUnit:surrogate.outcome.userId.charCodeAt(0),surrogateRaw:JSON.stringify({...base,account_number:lone}),uncheckedRaw:JSON.stringify({...base,account_number:undefined}),uncheckedType}));
})().catch(e=>{console.error(e);process.exitCode=1;});
