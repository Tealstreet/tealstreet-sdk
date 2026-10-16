const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
assert.ok(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const root=path.resolve(__dirname,'../../..');
function unused(name){return new Proxy(function(){throw Error('unused '+name);},{get(_t,key){if(key==='__esModule')return false;throw Error('unused '+name+'.'+String(key));}});}
function run(file,require){const exports={};const compiled=ts.transpileModule(fs.readFileSync(file,'utf8'),{fileName:file,reportDiagnostics:true,compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}});assert.equal(compiled.diagnostics.length,0);vm.runInNewContext(compiled.outputText,{exports,require},{filename:file});return exports;}
const types=run(path.join(root,'packages/safe-cex/src/types.ts'),id=>unused(id));
const constants=run(path.join(root,'packages/safe-cex/src/exchanges/backpack/backpack.types.ts'),id=>unused(id));
let xhrSetups=0,sockets=0;
class Base {createXhrWithSecondaryKey(){xhrSetups++;return unused('xhr');}}
class Socket {constructor(){sockets++;}}
const api=run(path.join(root,'packages/safe-cex/src/exchanges/backpack/backpack.exchange.ts'),id=>{
 if(id==='../../types')return types;
 if(id==='./backpack.types')return constants;
 if(id==='../base')return {BaseExchange:Base};
 if(id==='./backpack.api')return {createAPI:unused('createAPI')};
 if(id==='./backpack.ws-public')return {BackpackPublicWebSocket:Socket};
 if(id==='./backpack.ws-private')return {BackpackPrivateWebSocket:Socket};
 return unused(id);
});
const exchange=new api.BackpackExchange('backpack',{},{});
assert.equal(xhrSetups,1);assert.equal(sockets,2);
const statuses=['New','PartiallyFilled','Filled','Cancelled','Expired','','new','filled','Canceled','OPEN','λ🚀',' Filled','Filled\0'];
const backward=['GTC','IOC','FOK','','gtc','ioc','fok','PostOnly','ImmediateOrCancel','λ🚀',' IOC','IOC\0'];
const forward=[undefined,...Object.values(types.OrderTimeInForce)];
function capture(input,result){assert.equal(typeof result,'string');return {input:input===undefined?null:input,result};}
const status=statuses.map(s=>capture(s,exchange.mapOrderStatus(s)));
const tif=forward.map(s=>capture(s,exchange.mapTimeInForce(s)));
const reverse=backward.map(s=>capture(s,exchange.mapBackpackTimeInForce(s)));
const boundary=['\ud800','Filled\udc00'].map(s=>({raw:JSON.stringify(s),units:Array.from({length:s.length},(_,i)=>s.charCodeAt(i)),status:exchange.mapOrderStatus(s),reverse:exchange.mapBackpackTimeInForce(s)}));
process.stdout.write(JSON.stringify({runtime:{node:process.version,arch:process.arch,platform:process.platform},status,tif,reverse,boundary}));
