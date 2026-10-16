// Actual inverse mapper and actual Store construction/update, entirely offline.
import fs from 'node:fs';
import path from 'node:path';
import {createRequire} from 'node:module';
import {fileURLToPath} from 'node:url';
import {sourceStore} from '../../safe-cex-store/tests/store_source.mjs';
const require=createRequire(import.meta.url);
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../../..');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH || path.join(root,'node_modules/typescript'));
const source=file=>{
 let text=fs.readFileSync(path.join(root,file),'utf8');
 if(process.env.BITGET_INVERSE_MUTATION && file.endsWith('/bitgetinverse.exchange.ts')) {
  if(process.env.BITGET_INVERSE_MUTATION!=='boolean-reduce')throw Error('unknown inverse source mutation');
  const from="reduceOnly: o.reduceOnly?.toUpperCase?.() === 'YES',";
  if(text.split(from).length!==2)throw Error('inverse reduceOnly mutation anchor');
  text=text.replace(from,"reduceOnly: o.reduceOnly?.toUpperCase?.() === 'YES' || o.reduceOnly === true,");
 }
 return ts.createSourceFile(file,text,ts.ScriptTarget.Latest,true);
};
function evaluate(text,bindings={}) {const exports={};new Function('exports',...Object.keys(bindings),ts.transpileModule(text,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText)(exports,...Object.values(bindings));return exports;}
const domain=source('packages/safe-cex/src/types.ts');
const enums=evaluate(domain.statements.filter(ts.isEnumDeclaration).map(n=>n.getText(domain)).join('\n'));
const protocol=source('packages/safe-cex/src/exchanges/bitget/bitget.types.ts');
const tables=evaluate(protocol.statements.filter(n=>!ts.isImportDeclaration(n)).map(n=>n.getText(protocol)).join('\n'),enums);
const math=source('packages/safe-cex/src/utils/safe-math.ts');
const invalid=source('packages/math/src/isNotNumber.ts');
const arithmetic=evaluate(math.statements.filter(n=>!ts.isImportDeclaration(n)).map(n=>n.getText(math)).join('\n'),evaluate(invalid.getText()));
const src=source('packages/safe-cex/src/exchanges/bitget/bitgetinverse.exchange.ts');
const klass=src.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text==='BitgetInverseExchange');
const member=klass.members.find(n=>n.name?.getText(src)==='mapOrder');
if(!member)throw Error('missing inverse mapOrder');
let clockCalls=0;
const {Inverse}=evaluate('export class Inverse {'+member.getText(src)+'}',{...enums,...tables,...arithmetic,virtualClock:{getCurrentTimeMs:()=>{clockCalls++;return 1700000000123;}}});
const {Store}=sourceStore();
function open(value) {
 if(value===undefined)return {$undefined:true};
 if(typeof value==='number')return {$number:Object.is(value,-0)?'-0':String(value)};
 if(typeof value==='function')return {$function:value.name,$nativeMember:Object.getOwnPropertyNames(Object.prototype).find(k=>Object.getOwnPropertyDescriptor(Object.prototype,k).value===value)};
 if(value===Object.prototype)return {$prototype:'Object.prototype'};
 if(Array.isArray(value))return value.map(open);
 if(value&&typeof value==='object')return Object.fromEntries(Object.entries(value).map(([k,v])=>[k,open(v)]));
 return value;
}
const base={orderId:'x',status:'new',orderType:'limit',side:'buy',symbol:'BTCUSD',size:'3',price:'2'};
const cases=[];
function capture(name,patch={},inverse=true,product='coin-futures') {
 const input={...base,...patch},receiver=new Inverse(),store=new Store();
 const markets=inverse===null?[]:[{id:'btc',symbol:'BTCUSD',isInverse:inverse}];
 store.update({markets});
 const map=store.marketsBySymbol,calls=[];
 if(inverse!==null&&map.BTCUSD?.isInverse!==inverse)throw Error('production Store market setup failed');
 Object.defineProperty(store,'marketsBySymbol',{value:new Proxy(map,{get(target,key,receiver){calls.push(key);return Reflect.get(target,key,receiver);}})});
 receiver.store=store;receiver.apiProductType=product;clockCalls=0;
 let result;try{const output=receiver.mapOrder(input);result={value:open(output),serialized:JSON.stringify(output)};}catch(error){result={error:{name:error.name,message:error.message}};}
 cases.push({name,input:open(input),inverse,product,calls,clockCalls,result});
}
if(process.argv.includes('--field-identity'))capture('field-identity:all',{orderId:'identity-order',size:'13.5',price:'17.25',baseVolume:'2.75',presetStopLossPrice:'11.5',presetStopSurplusPrice:'23.75'});
for(const inverse of [null,false,true])for(const size of ['0','-0','0.00000001','-0.00000001','0.00000003','-0.00000003','3','-3','Infinity','-Infinity','NaN'])for(const price of ['0','-0','-2','2','Infinity','NaN'])capture(`round:${inverse}:${size}:${price}`,{size,price},inverse);
for(const kind of ['limit','market','UNKNOWN','pos_loss','pos_profit'])for(const side of ['buy','sell','close_long','constructor','UNKNOWN'])for(const mode of ['hedge_mode','one_way_mode',''])for(const reduce of ['YES','yes',true,false])capture(`mode:${kind}:${side}:${mode}:${reduce}`,{orderType:kind,side,posMode:mode,reduceOnly:reduce,stopLoss:'2',takeProfit:'3'});
for(const key of Object.getOwnPropertyNames(Object.prototype))for(const field of ['status','side','force'])
 capture(`deferred-refusal:${field}:${key}`,{[field]:key,presetStopLossPrice:'2',presetStopSurplusPrice:{toString:1}});
const values=[undefined,null,false,true,0,-0,12,1e21,'','0','-0','NaN','Infinity','-Infinity','0x10','12junk','\ufeff12.5','\u008512.5',[],[2],{}, {toString:1},{toUpperCase:3},{toUpperCase:null}];
for(const field of ['symbol','size','sz','price','px','baseVolume','filledQty','accFillSz','status','orderType','side','force','orderId','reduceOnly','triggerPrice','triggerPx'])for(let i=0;i<values.length;i++) {
 const prefix=field==='sz'?{size:undefined}:field==='px'?{price:undefined}:field==='triggerPrice'?{planType:'pos_loss',triggerPx:'8'}:field==='triggerPx'?{planType:'pos_loss',triggerPrice:'0'}:{};
 capture(`coercion:${field}:${i}`,{...prefix,[field]:values[i]});
}
for(const member of Object.getOwnPropertyNames(Object.prototype)) {
 for(const field of ['status','orderType','side','tradeSide','tS','force','planType'])capture(`prototype:${field}:${member}`,{[field]:member});
 for(const planType of ['pos_loss','pos_profit','UNKNOWN'])capture(`overwrite:${member}:${planType}`,{side:member,orderType:member,planType,posMode:'hedge_mode',triggerPrice:'0',triggerPx:'8'});
 capture(`inherited-market:${member}`,{symbol:member});
}
for(const keys of [['price','px'],['symbol','instId'],['size','sz'],['baseVolume','filledQty','accFillSz'],['orderId','ordId','id'],['orderType','ordType','planType'],['tradeSide','tS','side'],['status','state','planStatus'],['force','timeInForce','tiF'],['presetStopLossPrice','stopLossTriggerPrice'],['presetStopSurplusPrice','stopSurplusTriggerPrice']]) {
 const vs=keys[0]==='orderType'?['limit','market','pos_loss']:keys[0]==='tradeSide'?['buy','sell','close_long']:keys[0]==='status'?['new','filled','cancelled']:keys[0]==='force'?['gtc','fok','ioc']:keys.map((_,i)=>String(i+2));
 for(let mask=0;mask<2**keys.length;mask++)capture(`aliases:${keys[0]}:${mask}`,Object.fromEntries(keys.map((k,i)=>[k,mask&(1<<i)?vs[i]:null])));
}
for(const field of ['clientOid','orderStatus','qty','amount','cumExecQty','stopLoss','takeProfit','presetTakeProfitPrice'])capture(`excluded-main-alias:${field}`,{[field]:field==='orderStatus'?'new':'9',...(field==='clientOid'?{orderId:undefined}:field==='orderStatus'?{status:undefined}:['qty','amount'].includes(field)?{size:undefined,sz:undefined}:field==='cumExecQty'?{baseVolume:undefined,filledQty:undefined,accFillSz:undefined}:{})});
for(let i=0;i<values.length;i++)capture(`attached-id:${i}`,{orderId:values[i],presetStopLossPrice:'2',presetStopSurplusPrice:'3'});
for(const product of ['coin-futures','scoin-futures','İ','ſ','ΟΣ','𐐨','\ufeffcoin-futures'])capture(`product:${product}`,{symbol:`BTC_${product.toUpperCase()}_${product.toUpperCase()}`},true,product);
for(const symbol of ['\ufeffBTCUSD','\u0085BTCUSD','𐐀','\0'])capture(`unicode:${JSON.stringify(symbol)}`,{symbol});
capture('throw-priority:amount-before-status',{size:{toString:1},status:{toString:1},reduceOnly:{toUpperCase:3}});
capture('throw-priority:status-before-reduce',{status:{toString:1},reduceOnly:{toUpperCase:3}});
capture('throw-priority:reduce-before-force',{reduceOnly:{toUpperCase:3},force:{toString:1}});
capture('short-circuit:trigger-fallback-not-coerced',{planType:'pos_loss',triggerPrice:'7',triggerPx:{toString:1}});
capture('short-circuit:trigger-fallback-coerced',{planType:'pos_loss',triggerPrice:'0',triggerPx:{toString:1}});
const utf16=['\ud800','x\udc00'].map(symbol=>{const input={...base,symbol};const receiver=new Inverse();receiver.store=new Store();receiver.apiProductType='coin-futures';return {inputJson:JSON.stringify(input),symbolJson:JSON.stringify(receiver.mapOrder(input)[0].symbol)};});
const receiver=new Inverse();receiver.store=new Store();receiver.apiProductType='coin-futures';
const input={...base,label:'before',presetStopLossPrice:'2',presetStopSurplusPrice:'3'};const output=receiver.mapOrder(input);
input.label='after-input';input.nested={amount:NaN};const afterInput=open(output);
output[1].info.label='after-output';const alias={sameReference:output.every(order=>order.info===input),afterInput,afterOutput:open(input)};
const extraAlias={sameReference:output.every(order=>order.extra===output[0].extra)};
output[0].extra.remainingNative=17;extraAlias.afterPrimary=open(output.map(order=>order.extra));
output[1].extra.remainingNative=NaN;extraAlias.afterAttached=open(output.map(order=>order.extra));
const numericInfoInput={...base,nested:{amount:NaN}};
const numericInfo={input:open(numericInfoInput),output:open(receiver.mapOrder(numericInfoInput)[0])};
if(process.argv.includes('--finding-policy')) {
 const members=['constructor','__defineGetter__','__defineSetter__','hasOwnProperty','__lookupGetter__','__lookupSetter__','isPrototypeOf','propertyIsEnumerable','toString','valueOf','__proto__','toLocaleString'];
 const prototype=members.map(key=>{
  const record=receiver.mapOrder({...base,status:key})[0];
  const expected=key==='__proto__'?Object.prototype:Object.prototype[key];
  if(record.status!==expected)throw Error('native inverse inherited status identity BEFORE serialization');
  return {key,kind:typeof record.status};
 });
 const utf16=['\ud800','x\udc00'].map(symbol=>{
  const value=receiver.mapOrder({...base,symbol})[0].symbol;
  const units=Array.from({length:value.length},(_,i)=>value.charCodeAt(i));
  const expected=symbol==='\ud800'?[0xd800]:[0x78,0xdc00];
  if(units.length!==expected.length||units.some((v,i)=>v!==expected[i]))throw Error('native inverse symbol units BEFORE serialization');
  return {units,escaped:JSON.stringify(symbol)};
 });
 const input={...base,nested:{amount:NaN}};const record=receiver.mapOrder(input)[0];
 if(record.info!==input||!Number.isNaN(input.nested.amount)||!Number.isNaN(record.info.nested.amount))throw Error('native inverse info accepts NaN before projection');
 if(prototype.length!==12||utf16.length!==2)throw Error('independent inverse policy floor');
 process.stdout.write(JSON.stringify({prototype,utf16,nativeInfoNaN:true}));process.exit(0);
}
process.stdout.write(JSON.stringify({cases,utf16,alias,extraAlias,numericInfo}));
