import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {fileURLToPath} from 'node:url';
const require=createRequire(import.meta.url);
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../../..');
const read=p=>ts.createSourceFile(p,fs.readFileSync(path.join(root,'packages/safe-cex/src',p),'utf8'),ts.ScriptTarget.Latest,true);
const source=read('exchanges/kucoin/kucoin.exchange.ts');
const cls=source.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text==='KucoinExchange');
const names={all:'cancelAllOrders',dispatch:'cancelOrders',normal:'cancelNormalOrders',symbol:'cancelSymbolOrders',update:'updateOrder',algo:'updateAlgoOrder',leverage:'setLeverage',margin:'changeMarginType',mode:'changePositionMode'};
const transpile=s=>ts.transpileModule(s,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
function declaration(src,name){const n=src.statements.find(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>d.name.getText(src)===name));assert.ok(n,name);return n.getText(src);}
function module(text){const m={exports:{}};vm.runInNewContext(transpile(text),{module:m,exports:m.exports});return m.exports;}
const ENDPOINTS=module(declaration(read('exchanges/kucoin/kucoin.types.ts'),'ENDPOINTS')).ENDPOINTS;
const types=read('types.ts');
const {MarginType}=module(types.statements.find(n=>ts.isEnumDeclaration(n)&&n.name.text==='MarginType').getText(types));
const partition=require('lodash/partition'),uniqBy=require('lodash/uniqBy');
const pack=v=>{if(v===undefined)return {u:true};if(typeof v==='number'){if(Number.isNaN(v))return {n:'nan'};const b=Buffer.alloc(8);b.writeDoubleBE(v);return {n:b.toString('hex')};}if(Array.isArray(v))return v.map(pack);if(v&&typeof v==='object')return {o:Object.entries(v).map(([k,v])=>[k,pack(v)])};return v;};
async function fixture(method,c){
 const events=[],freshErrors=new Set();
 const original=Object.assign(new TypeError('DOWN'),{code:'WRITE',ids:[8,6],response:{status:c.startsWith('retry-')?404:c==='string-status'?'404':500,data:{msg:'VENUE'}}});
 function TracedError(message){events.push(['newError',pack(message)]);const e=new Error(message);freshErrors.add(e);return e;}
 const initial={id:'ROOT__tp',symbol:'SYM',side:'sell',type:c==='algo'||method==='algo'?'stop_market':'limit',price:11,amount:4,timeInForce:'IOC',reduceOnly:false,tag:29};
 const update=c==='no-fields'?{}:c==='undefined'?{price:undefined,amount:undefined}:{price:17,amount:2};
 const sibling={id:'ROOT__sl',symbol:'OTHER'},other={id:'SECOND',symbol:'OTHER'};
 const orders=method==='dispatch'||method==='normal'?[initial,other,sibling]:c==='virtual'?[initial,sibling]:[initial];
 const market={id:'INST',limits:{leverage:{min:1,max:9}}};
 const positions=[{contracts:c==='string-zero'?'0':c==='open'?1:0}],refreshed=[{id:'REFRESHED'}];
 const receiver={store:{orders,marketsBySymbol:c==='missing'?{}:{SYM:market}}};
 let emits=0,fetches=0,requests=0;
 receiver.emitAndLogError=(...args)=>{events.push(['emit',...args.map(pack)]);if(c==='emit-reject'&&++emits===1)throw original;};
 receiver.toApiSymbol=symbol=>`API-${symbol}`;
 receiver.isVirtualTPSLOrder=order=>order.id.includes('__');
 receiver.isAlgoOrder=order=>order.type!=='limit';
 receiver.nativeAmountForCancelReplace=order=>{assert.equal(order,initial);events.push(['nativeAmount']);return 40;};
 receiver.getOrFetchMarginType=async symbol=>{events.push(['margin',pack(symbol)]);return c==='isolated'?MarginType.Isolated:MarginType.Cross;};
 receiver.fetchPositions=async(...args)=>{fetches++;events.push(['fetch',...args.map(pack)]);if(c==='refresh-reject'&&(method!=='mode'||fetches===2))throw original;return method==='mode'&&fetches===1?positions:refreshed;};
 receiver.store.addOrUpdatePositions=value=>{assert.equal(value,refreshed);events.push(['this.store.addOrUpdatePositions',pack(value)]);};
 receiver.store.update=value=>{assert.equal(value.positions,refreshed);events.push(['this.store.update',pack(value)]);};
 async function request(verb,...args){requests++;events.push([verb,...args.map(pack)]);const retry=c.startsWith('retry-');if(c==='reject'||c==='string-status'||retry&&(requests===1||c==='retry-reject'&&requests===2))throw original;const bad=c==='bad'||c==='bad-empty'||c==='emit-reject'||c==='retry-bad'&&requests===2;return {data:{code:bad?'400000':'200000',msg:c==='bad-empty'?'':bad?'DENIED':''}};}
 receiver.unlimitedXHR={delete:(...args)=>request('delete',...args)};
 receiver.xhr={post:(...args)=>request('post',...args)};
 const fakeMath=Object.create(Math);
 if(c==='callee-reenter')fakeMath.max=(a,b)=>{fakeMath.min=()=>777;market.limits.leverage.max=7;return Math.max(a,b);};
 const context={ENDPOINTS,MarginType,partition,uniqBy,Error:TracedError,Math:fakeMath};
 for(const name of Object.values(names)){const member=cls.members.find(n=>n.name?.getText(source)===name);assert.ok(member,name);receiver[name]=vm.runInNewContext(transpile(`(function(){return (${member.initializer.getText(source)});})`),context).call(receiver);}
 const selected=names[method];
 const delegates={
  updateVirtualTpSlOrder:async(opts,related)=>{assert.equal(opts.order,initial);assert.equal(opts.update,update);assert.equal(related[0],initial);assert.equal(related[1],sibling);events.push(['this.updateVirtualTpSlOrder',pack(opts),pack(related)]);return ['DELEGATE'];},
  updateAlgoOrder:async opts=>{assert.equal(opts.order,initial);assert.equal(opts.update,update);events.push(['this.updateAlgoOrder',pack(opts)]);return ['DELEGATE'];},
  cancelNormalOrders:async rows=>{assert.equal(rows[0],other);assert.equal(rows[1],initial);assert.equal(rows[2],sibling);events.push(['this.cancelNormalOrders',pack(rows)]);if(c==='reject')throw original;},
  cancelOrders:async rows=>{assert.equal(rows.length,1);assert.equal(rows[0],initial);events.push(['this.cancelOrders',pack(rows)]);if(c==='cancel-reject')throw original;},
  placeOrder:async payload=>{events.push(['this.placeOrder',pack(payload)]);if(c==='place-reject')throw original;return ['PLACED'];},
 };
 for(const[name,binding]of Object.entries(delegates))if(name!==selected)receiver[name]=binding;
 const args=method==='all'?[]:method==='dispatch'||method==='normal'?[orders]:method==='symbol'?['SYM']:method==='update'||method==='algo'?[{order:initial,update}]:method==='leverage'?['SYM',50]:method==='margin'?[c!=='cross',c==='no-symbol'?undefined:'SYM']:[c!=='oneway'];
 let result;
 try{result={ok:pack(await receiver[selected](...args))};}
 catch(e){assert.ok(e===original||freshErrors.has(e),`${method}/${c}: ${e.stack}`);result={error:{name:e.name,message:e.message,fresh:freshErrors.has(e),code:e.code??null,ids:e.ids??null}};}
 return {method,case:c,result,events};
}
let input='';for await(const part of process.stdin)input+=part;
const outputs=[];for(const c of JSON.parse(input).cases)outputs.push(await fixture(c.method,c.case));
assert.equal(outputs.length,48);process.stdout.write(JSON.stringify(outputs));
