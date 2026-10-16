// Actual source bodies and actual library primitives, with native host effects.
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict'),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const root=path.resolve(__dirname,'../../..'),directory=path.join(root,'packages/safe-cex/src/exchanges/hyperliquid');
const units=s=>Array.from({length:s.length},(_,i)=>s.charCodeAt(i)),bits=n=>{if(Number.isNaN(n))return 'nan';const b=Buffer.alloc(8);b.writeDoubleBE(n);return b.toString('hex');};
let recording=false,depth=0,calls=[],errors=new Map(),nextError=3,special=null,parsedHandle;
const sentinels=[null,{sentinel:1},{sentinel:2}],unproxy=new WeakMap(),classes=new WeakMap();
function native(v){if(v===undefined)return ['undefined'];if(v===null)return ['null'];if(typeof v==='number')return ['number',bits(v)];if(typeof v==='string')return ['string',units(v)];if(typeof v==='boolean')return ['bool',v];if(typeof v==='bigint')return ['opaque','bigint'];if(typeof v==='symbol')return ['opaque','symbol'];if(typeof v==='function')return ['opaque','function'];v=unproxy.get(v)||v;
 if(classes.has(v)||Object.prototype.toString.call(v)==='[object Error]'||Object.prototype.toString.call(v)==='[object DOMException]')return ['error',classes.get(v)||'builtin:'+v.name,units(v.name),units(v.message),Object.hasOwn(v,'message'),Object.hasOwn(v,'name'),Object.hasOwn(v,'code'),native(v.code),Object.hasOwn(v,'code')?(()=>{const d=Object.getOwnPropertyDescriptor(v,'code');return [d.writable,d.enumerable,d.configurable];})():null];
 if(Array.isArray(v))return ['array',v.map(native)];return ['object',Object.keys(v).map(k=>{const d=Object.getOwnPropertyDescriptor(v,k);return [units(k),Object.hasOwn(d,'value')?native(d.value):['opaque','accessor']];})];
}
function errorId(e){const i=sentinels.indexOf(e);if(i>0)return i;if(!errors.has(e))errors.set(e,nextError++);return errors.get(e);}
function invoke(name,arg,action,project=native){const before=depth;depth++;try{const v=action();if(recording&&before===0)calls.push({name,arg,result:project(v)});return v;}catch(e){if(recording&&before===0)calls.push({name,arg,error:{id:errorId(e),native:native(e)}});throw e;}finally{depth--;}}
const constructors={};
const ErrorBinding=new Proxy(Error,{construct(target,args,newTarget){const cls=newTarget===ErrorBinding?'intrinsic':Object.keys(constructors).find(k=>constructors[k]===newTarget);assert(cls,'known derived source Error class');return invoke('newError',{class:cls,message:native(args[0])},()=>{if(special?.newError)throw sentinels[1];const error=Reflect.construct(target,args,newTarget);classes.set(error,cls);const p=new Proxy(error,{defineProperty(target,key,descriptor){return invoke('define:'+key,{target:native(target),value:native(descriptor.value)},()=>{if(special?.defineError)throw sentinels[1];return Reflect.defineProperty(target,key,descriptor);});},set(target,key,value){return invoke('set:'+key,{target:native(target),value:native(value)},()=>{if(special?.setError===key)throw sentinels[1];return Reflect.set(target,key,value,target);});}});unproxy.set(p,error);return p;});}});
function propertyTransformer(context){const visit=node=>{
 if(ts.isTemplateSpan(node)&&ts.isPropertyAccessExpression(node.expression)&&node.expression.name.text==='length')return ts.factory.updateTemplateSpan(node,ts.factory.createCallExpression(ts.factory.createIdentifier('__implicit'),undefined,[ts.visitNode(node.expression,visit)]),node.literal);
 if(ts.isVariableDeclaration(node)&&ts.isArrayBindingPattern(node.name)&&node.name.elements.length===2)return ts.factory.updateVariableDeclaration(node,node.name,node.exclamationToken,node.type,ts.factory.createCallExpression(ts.factory.createIdentifier('__firstTwo'),undefined,[node.initializer]));
 if(ts.isPropertyAccessExpression(node)&&!['LogSeverity','Array'].includes(node.expression.getText())&&!(ts.isBinaryExpression(node.parent)&&node.parent.left===node&&node.parent.operatorToken.kind===ts.SyntaxKind.EqualsToken))return ts.factory.createCallExpression(ts.factory.createIdentifier(node.questionDotToken?'__optionalGet':'__propertyGet'),undefined,[ts.visitNode(node.expression,visit),ts.factory.createStringLiteral(node.name.text)]);
 return ts.visitEachChild(node,visit,context);
};return source=>ts.visitNode(source,visit);}
function compile(source,context,transform=false){vm.runInNewContext(ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022},transformers:transform?{before:[propertyTransformer]}:undefined}).outputText,context);return context.exports;}
function loadError(file,tag){const context={exports:{},Error:ErrorBinding,__propertyGet:(v,key)=>invoke('get:'+key,native(v),()=>v[key])};const exports=compile(fs.readFileSync(path.join(directory,file),'utf8'),context,true);constructors[tag]=exports.HyperliquidAPIError;return exports;}
const core=loadError('hyperliquid.errors.ts','core'),spot=loadError('hyperliquidspot.errors.ts','spot');
function selected(file,names){const s=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);return names.map(name=>{const node=s.statements.find(n=>ts.isFunctionDeclaration(n)&&n.name.text===name||ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>d.name.text===name));assert(node,'actual source '+name);return node.getText(s)+`\nexports.${name}=${name};`;}).join('\n');}
const ccxt=compile(selected(path.join(root,'packages/ccxt-compat/src/functions/ccxt_exchange.ts'),['numberToString','parseToInt']),{exports:{}});
const type=compile(fs.readFileSync(path.join(root,'packages/ccxt-compat/src/functions/type.ts'),'utf8'),{exports:{}});
const precise=compile(fs.readFileSync(path.join(root,'packages/ccxt-compat/src/Precise.ts'),'utf8'),{exports:{}}).default;
const getKey=compile(fs.readFileSync(path.join(root,'packages/safe-cex/src/utils/get-key.ts'),'utf8'),{exports:{},require:id=>{const value=require(id);return {__esModule:true,default:value.default??value};}}).v;
const primitives={
 numberToString:n=>invoke('numberToString',native(n),()=>special?.noNumberString?undefined:ccxt.numberToString(n)),
 parseToInt:n=>invoke('parseToInt',native(n),()=>ccxt.parseToInt(n)),
};
const utilsContext={exports:{},require:id=>{if(id.endsWith('ccxt_exchange'))return primitives;if(id.endsWith('/type'))return {safeString:type.safeString};return {__esModule:true,default:{stringEq:(a,b)=>invoke('precise:Equal',[units(a),b],()=>precise.stringEq(a,b)),stringGt:(a,b)=>invoke('precise:Greater',[units(a),b],()=>precise.stringGt(a,b)),stringLt:(a,b)=>invoke('precise:Less',[units(a),b],()=>precise.stringLt(a,b))}};},__lower:(receiver,fn)=>invoke('lowercase',units(String(receiver)),()=>Reflect.apply(fn,receiver,[]))};
compile(fs.readFileSync(path.join(directory,'hyperliquid.utils.ts'),'utf8'),utilsContext);
vm.runInNewContext('const originalLower=String.prototype.toLowerCase;String.prototype.toLowerCase=function(){return __lower(this,originalLower);};',utilsContext);
const utility=utilsContext.exports;
const exchange=compile(selected(path.join(directory,'hyperliquid.exchange.ts'),['parseFiniteNumber','parseHyperliquidVolumeFields','parseHyperliquidOpenInterestNotional']),{exports:{},v:(v,k)=>invoke('v:'+k,native(v),()=>getKey(v,k)),String:v=>invoke('string',native(v),()=>String(v)),parseFloat:s=>invoke('parseFloat',units(s),()=>parseFloat(s))});
const pako=require('pako');
const fast=compile(fs.readFileSync(path.join(directory,'hyperliquid.fast-asset-ctxs.ts'),'utf8'),{exports:{},Error:ErrorBinding,require:id=>{assert.equal(id,'pako');return {inflateRaw:bytes=>invoke('inflateRaw',Array.from(bytes),()=>pako.inflateRaw(bytes),v=>['bytes',Array.from(v)])};},atob:s=>invoke('atob',units(s),()=>special?.atobUnicode??atob(s)),TextDecoder:class {decode(bytes){return invoke('decodeText',Array.from(bytes),()=>new TextDecoder().decode(bytes));}},JSON:{parse:s=>invoke('parse',units(s),()=>{parsedHandle=JSON.parse(s);return parsedHandle;})},Array:{isArray:v=>invoke('isArray',native(v),()=>Array.isArray(v))}});
const severity=compile(fs.readFileSync(path.join(root,'packages/logging/src/LogSeverity.ts'),'utf8'),{exports:{}}).LogSeverity;
let lastPair;
const tupleContext={exports:{},__implicit:v=>invoke('implicitString',native(v),()=>`${v}`),LogSeverity:severity,__propertyGet:(v,k)=>invoke('get:'+k,native(v),()=>v[k]),__optionalGet:(v,k)=>v==null?undefined:invoke('get:'+k,native(v),()=>v[k]),__firstTwo:v=>invoke('firstTwo',native(v),()=>{const [a,b]=v;lastPair=[a,b];return [a,b];}),Array:{isArray:v=>invoke('isArray',native(v),()=>Array.isArray(v))}};
const tuple=compile(selected(path.join(directory,'hyperliquid.market-startup-helpers.ts'),['parseMetaAndAssetCtxsResponse']),tupleContext,true);
function logger(message,level){return invoke('warning',[units(message),level],()=>{if(special?.logError)throw sentinels[1];});}
const functions=['core.class','core.constructor','core.handleApiError','spot.class','spot.constructor','spot.handleApiError','symbolToCoin','calculatePricePrecision','parseHyperliquidVolumeFields','parseHyperliquidOpenInterestNotional','decodeHyperliquidFastAssetCtxs','parseMetaAndAssetCtxsResponse'];
const cases=[];
function add(name,args,config=null,label='base'){recording=false;calls=[];errors=new Map();nextError=3;special=config;parsedHandle=undefined;lastPair=undefined;const input=args.map(native);recording=true;let value,error;
 try{if(name.startsWith('core.')||name.startsWith('spot.')){const source=name.startsWith('core.')?core:spot;value=name.endsWith('handleApiError')?source.handleApiError(...args):new source.HyperliquidAPIError(...args);}else{const source=utility[name]?utility:exchange[name]?exchange:tuple[name]?tuple:fast;value=source[name](...args);}}
 catch(e){error={id:errorId(e),native:native(e)};}
 recording=false;cases.push({name,label,input,calls,...error?{error}:{native:native(value),...name==='decodeHyperliquidFastAssetCtxs'?{sameParsed:value===parsedHandle}:name==='parseMetaAndAssetCtxsResponse'&&value?{sameMembers:value[0]===lastPair[0]&&value[1]===lastPair[1]}:{}}});
}
const pairs=[['CODE','message'],['\ud800','\udfff'],['',''],[false,undefined],[0,null],[{x:'code'},{x:'message'}],[Symbol('code'),Symbol('message')],[undefined,-0],[7n,42],[NaN,true]];assert.equal(pairs.length,10);
for(const name of ['core.class','core.constructor','spot.class','spot.constructor'])for(const pair of pairs)add(name,pair);
const inputs=[undefined,null,false,0,-0,NaN,'x','',Symbol('x'),7n,function input(){},[],{},{message:'setup'},{request:true},{response:{data:{code:'CODE',message:'MESSAGE'}}},{response:{data:{code:0,message:false}}},{response:{data:{code:{x:'code'},message:'\ud800'}}}];assert.equal(inputs.length,18);
for(const name of ['core.handleApiError','spot.handleApiError'])for(const input of inputs)add(name,[input]);
for(const name of ['core.handleApiError','spot.handleApiError']){
 for(const input of [{get response(){throw sentinels[1];}},{response:true},{response:{data:null}},{get request(){throw sentinels[1];}},{get message(){throw sentinels[1];}},{response:{data:{get code(){throw sentinels[1];}}}},{response:{data:{get message(){throw sentinels[2];}}}}])add(name,[input],null,'read/opaque errors');
 let read=0;const response={get response(){read++;return read===1?{}:read===2?{data:{code:'FRESH'}}:{data:{message:'\ud800fresh'}};}};add(name,[response],null,'fresh response/data reads');
}
for(const name of ['core.class','core.constructor','spot.class','spot.constructor'])for(const config of [{newError:true},{defineError:true},{setError:'code'},{setError:'name'}])add(name,['CODE','MESSAGE'],config,'constructor primitive errors');
assert.equal(cases.length,108,'independent40constructor+36handler+16effects+16constructorerrors');
const symbols=['BTC-PERP','XYZ|NQ-PERP','XYZ|NQ-perp','a|b|c-PERP','|ASSET','DEX|','DEX|ASSET-PERP\n','DEX|ASSET-PERP\r','\ud800|\udfff-PERP','İΟΣ|Asset-PerP','K|雪-PERP','a|b-PERP\u2028'];assert.equal(symbols.length,12);for(const s of symbols)add('symbolToCoin',[s]);
const prices=[0,-0,.1,.000001,17.5,91.25,123456,1e-7,1e21,-.5,Number.MIN_VALUE,NaN,Infinity,-Infinity];assert.equal(prices.length,14);for(const price of prices)for(const [amount,max] of [[2,6],[8,6],[-0,-0],[NaN,6]])add('calculatePricePrecision',[price,amount,max]);
add('calculatePricePrecision',[17.5,2,6],{noNumberString:true},'undefined formatter');
const tickers=[null,{}, {dayNtlVlm:'182.5',dayBaseVlm:'17.5',openInterest:'3'}, {day_ntl_vlm:'91.25',day_base_vlm:'0',open_interest:'5'}, {dayNtlVlm:-0,dayBaseVlm:-0,openInterest:-0}, {dayNtlVlm:'\ufeff12tail',dayBaseVlm:'-.25x',openInterest:'Infinity'}, {dayNtlVlm:Infinity,dayBaseVlm:NaN,openInterest:-Infinity}, {dayNtlVlm:7n,dayBaseVlm:true,openInterest:Symbol('x')}];assert.equal(tickers.length,8);for(const name of ['parseHyperliquidVolumeFields','parseHyperliquidOpenInterestNotional'])for(const ticker of tickers)for(const price of [2,-0,Infinity])add(name,[ticker,price]);
for(const name of ['parseHyperliquidVolumeFields','parseHyperliquidOpenInterestNotional'])for(const ticker of [{get dayNtlVlm(){throw sentinels[1];},get openInterest(){throw sentinels[1];}},{dayNtlVlm:{toString(){throw sentinels[2];}},openInterest:{toString(){throw sentinels[2];}}}])add(name,[ticker,2],null,'lookup/coercion errors');
const compressed=bytes=>Buffer.from(pako.deflateRaw(bytes)).toString('base64');
const jsons=['{}','{"X":{"markPx":"91.25","midPx":"17.5"}}','{"\\ud800":{"markPx":"\\udfff"}}','{"markPx":false,"unknown":[-0,1e400]}','null','[]','0','false','"x"'];assert.equal(jsons.length,9);
for(const input of [undefined,null,false,0,{},[],'','!','AAAA'])add('decodeHyperliquidFastAssetCtxs',[input]);
for(const json of jsons)add('decodeHyperliquidFastAssetCtxs',[compressed(Buffer.from(json))]);
add('decodeHyperliquidFastAssetCtxs',[compressed(Buffer.from([0xef,0xbb,0xbf,...Buffer.from('{"x":"BOM"}')]))],null,'BOM decode');
add('decodeHyperliquidFastAssetCtxs',[compressed(Buffer.from([0x7b,0x22,0x78,0x22,0x3a,0x22,0xff,0x22,0x7d]))],null,'nonfatal UTF8 replacement');
add('decodeHyperliquidFastAssetCtxs',[compressed(Buffer.from('{bad'))],null,'JSON syntax error');
assert.equal(cases.length,250,'independent108+12+56+1+48+4+21case floor');
const tupleValues=[undefined,null,false,0,NaN,'x',{},function value(){},Symbol('x'),7n,[],[{}],[{universe:[]}],[{universe:[]},[]],[{universe:[]},[],'extra'],[null,[]],[0,[]],[{universe:null},[]],[{get universe(){throw sentinels[1];}},[]],[true,[]]];
assert.equal(tupleValues.length,20);
for(const value of tupleValues)add('parseMetaAndAssetCtxsResponse',[value,'primary','core markets',logger]);
const custom=[];custom[Symbol.iterator]=function*(){yield {universe:[]};yield [];throw Error('must close before third value');};add('parseMetaAndAssetCtxsResponse',[custom,'custom','context',logger],null,'custom iterator close');
const throwing=[];throwing[Symbol.iterator]=()=>{throw sentinels[1];};add('parseMetaAndAssetCtxsResponse',[throwing,'primary','context',logger],null,'iterator error');
add('parseMetaAndAssetCtxsResponse',[null,'primary','context',logger],{logError:true},'logger error');
add('parseMetaAndAssetCtxsResponse',[[],'primary','context',logger],{logError:true},'shape logger error');
add('parseMetaAndAssetCtxsResponse',[null,'\ud800','\udfff',logger],null,'UTF16 labels');
assert.equal(cases.length,275,'independent250+25tuple cases');
assert.equal(cases[0].native[1],'core','actual derived Error prototype');
assert.deepEqual(cases[120].native,['number','4010000000000000'],'actual finite ccxt/Precise result, not import failure');
console.log(JSON.stringify({functions,cases,iterationStop:274,scope:'12new source symbols; actual library primitive results and native receiver/error/field effects; intrinsic implementations remain host bindings'}));
