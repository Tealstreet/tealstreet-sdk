import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {fileURLToPath} from 'node:url';
const ts=createRequire(import.meta.url)(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../../..');
const files={KucoinExchange:'kucoin.exchange',KucoinSpotExchange:'kucoinspot.exchange',KucoinPrivateWebsocket:'kucoin.ws-private',KucoinPublicWebsocket:'kucoin.ws-public',KucoinSpotPrivateWebsocket:'kucoinspot.ws-private',KucoinSpotPublicWebsocket:'kucoinspot.ws-public'};
const source=(file)=>ts.createSourceFile(file,fs.readFileSync(path.join(root,'packages/safe-cex/src/exchanges/kucoin',file+'.ts'),'utf8'),ts.ScriptTarget.Latest,true);
const transpile=s=>ts.transpileModule(s,{compilerOptions:{target:ts.ScriptTarget.ESNext,module:ts.ModuleKind.CommonJS,useDefineForClassFields:true}}).outputText;
const pack=v=>{if(v===undefined)return{u:true};if(typeof v==='number'){if(Number.isNaN(v))return{n:'nan'};const b=Buffer.alloc(8);b.writeDoubleBE(v);return{n:b.toString('hex')};}if(typeof v==='function')return v.oracleCall?{call:v.oracleCall}:{fn:v.name};if(v instanceof RegExp||Object.prototype.toString.call(v)==='[object RegExp]')return{regexp:v.source};if(Array.isArray(v))return Array.from(v,pack);if(v&&typeof v==='object')return{o:Object.entries(v).map(([k,v])=>[k,pack(v)])};return v;};
async function fixture(name,c){
 const original=Object.assign(new TypeError('DOWN'),{code:'BOOT',ids:[3,7],response:{data:{msg:'VENUE'}}});
 const events=[];let instance,epoch=1;
 const opts=c==='null-options'?{role:null,shardIndex:null,shardCount:null}:c==='options'?{role:'snapshot',shardIndex:2,shardCount:4}:{testnet:false};
 const exchange=name.endsWith('Exchange'),args=exchange?['NAME',opts,{tag:'STORE'},'EXTRA']:[{tag:'PARENT'},opts,'EXTRA'];
 const record=(name,args)=>events.push([name,...args]);
 const base=(label)=>({[label]:class{constructor(...params){record('super',[label,new.target.name,params]);if(c==='super-reject')throw original;
  const target=exchange?{baseKind:'exchange',name:params[0],options:params[1],store:params[2]}:{baseKind:'socket',parent:params[0]};
  Object.setPrototypeOf(target,new.target.prototype);
  instance=new Proxy(target,{defineProperty(target,key,descriptor){record('define',[key,descriptor.value]);if(c==='define-reject')throw original;return Reflect.defineProperty(target,key,descriptor);},set(target,key,value){record('set',[key,value]);return Reflect.defineProperty(target,key,{value,enumerable:true,writable:true,configurable:true});}});
  return instance;
 }}})[label];
 const baseName=exchange?'BaseExchange':'BaseWebSocket';let Base=base(baseName);
 Base.prototype.createXhrWithSecondaryKey=function(...params){assert.equal(this,instance);assert.equal(params[0],opts);record('secondary',params);if(c==='secondary-reject')throw original;
  const value=params[2].rateLimit({raw:'XHR'});if(c==='reenter'){epoch=2;context.createAPI=api('createAPI',2);context.createKucoinSpotAPI=api('createKucoinSpotAPI',2);opts.testnet=true;}return value;
 };
 if(c==='prototype-reject')Base=new Proxy(Base,{get(target,key){if(key==='prototype')throw original;return Reflect.get(target,key);}});
 const api=(label,i)=>{const value=function(...params){assert.equal(params[0],opts);assert.equal(params[1],instance);record('api',[label+':'+i,params[0]]);if(c==='unlimited-reject')throw original;return{api:label+':'+i};};value.oracleCall=label+':'+i;return value;};
 const socket=(label)=>({[label]:class{constructor(...params){assert.equal(params[0],instance);record('new',[label,params]);if(c===(label.includes('Public')?'public-reject':'private-reject'))throw original;return{socket:label};}}})[label];
 const context={BaseExchange:Base,BaseWebSocket:Base,
  GRAFANA_ENDPOINTS_EVERY:['EVERY'],GRAFANA_ENDPOINTS_PERIODIC:['PERIODIC'],KUCOIN_SPOT_GRAFANA_ENDPOINTS_EVERY:['EVERY'],KUCOIN_SPOT_GRAFANA_ENDPOINTS_PERIODIC:['PERIODIC'],
  patternClientOrderIdRule:(label,pattern)=>{record('rule',[label,pattern]);return{label,requirement:`match ${pattern}`,isValid:(clientOrderId)=>pattern.test(clientOrderId)};},
  rateLimit:(...params)=>{record('rate',params);if(c==='rate-reject')throw original;return{limited:params[0],maxRPS:params[1].maxRPS};},
  createAPI:api('createAPI',1),createKucoinSpotAPI:api('createKucoinSpotAPI',1),
  KucoinPublicWebsocket:socket('KucoinPublicWebsocket'),KucoinPrivateWebsocket:socket('KucoinPrivateWebsocket'),KucoinSpotPublicWebsocket:socket('KucoinSpotPublicWebsocket'),KucoinSpotPrivateWebsocket:socket('KucoinSpotPrivateWebsocket'),
 };
 const src=source(files[name]),cls=src.statements.find(ts.isClassDeclaration);assert.equal(cls.name.text,name);
 let definition=null,result;
 try{
  const m={exports:{}};Object.assign(context,{module:m,exports:m.exports});
  const klass=vm.runInNewContext(transpile(cls.getText(src)+'\nmodule.exports='+name),context);
  if(c==='reparent'){const Other=base('OtherBase');Other.prototype.createXhrWithSecondaryKey=Base.prototype.createXhrWithSecondaryKey;Object.setPrototypeOf(klass,Other);}
  const descriptors=Object.getOwnPropertyDescriptors(klass.prototype);
  definition={name:klass.name,length:klass.length,base:Object.getPrototypeOf(klass).name,prototype:Object.entries(descriptors).filter(([key])=>key!=='constructor').map(([key,d])=>({name:key,kind:d.get?'get':'method',length:(d.get||d.value).length,function:pack(d.get||d.value)}))};
  const target=c==='subclass'?({Derived:class extends klass{}}).Derived:klass;
  result={ok:pack(Reflect.construct(klass,args,target))};
 }catch(e){assert.equal(e,original,`${name}/${c}: ${e.stack}`);result={error:{name:e.name,message:e.message,code:e.code,ids:e.ids}};}
 return{class:name,case:c,definition,result,events:events.map(row=>row.map(pack))};
}
let input='';for await(const part of process.stdin)input+=part;
const rows=[];for(const c of JSON.parse(input).cases)rows.push(await fixture(c.class,c.case));
assert.equal(rows.length,47,'independent construction fixture floor');process.stdout.write(JSON.stringify(rows));
