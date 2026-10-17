// Execute actual worker handlers and inspect the receiving structured clone.
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const throttle=require(path.join(process.env.SAFE_CEX_WORKER_LODASH_PATH,'throttle.js'));
const root=path.resolve(__dirname,'../../..');
function body(name){const p=path.join(root,'packages/safe-cex/src/webworker/handlers',name+'.ts');const s=fs.readFileSync(p,'utf8'),ast=ts.createSourceFile(p,s,ts.ScriptTarget.Latest,true);return ast.statements.filter(n=>!ts.isImportDeclaration(n)).map(n=>n.getText(ast)).join('\n').replace(/\bexport /g,'');}
const context=vm.createContext({console:{log(){},warn(){},error(){}},setTimeout:()=>1,clearTimeout(){},throttle,Error,Date});
vm.runInContext(ts.transpileModule(['BaseHandler','ThrottleManager','CallMethodHandler','OHLCVSubscriptionsHandler'].map(body).join('\n')+'\nglobalThis.Handlers={CallMethodHandler,OHLCVSubscriptionsHandler};',{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);
const candles=[{timestamp:1,open:NaN,high:Infinity,low:-Infinity,close:-0,volume:7.25},{timestamp:2,open:13.5,high:27.75,low:3.125,close:17.875,volume:NaN,lastSide:'sell'}];
const order={id:'retained',symbol:'BTC',status:'open',type:'limit',side:'buy',price:undefined,amount:NaN,filled:-0,remaining:Infinity,reduceOnly:false,lastUpdate:1,timeInForce:undefined};
function observe(v){if(v===undefined)return {kind:'undefined'};if(typeof v==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(Number.isNaN(v)?NaN:v);return {kind:'number',bits:b.toString('hex')};}if(Array.isArray(v))return v.map(observe);if(v!==null&&typeof v==='object')return Object.fromEntries(Object.keys(v).map(k=>[k,observe(v[k])]));return v;}
(async()=>{
 const records=[],posts=[];let callback;
 const exchange={name:'bybit',store:{markets:[{symbol:'BTC'}]},options:{},fetchOHLCV:async()=>candles,fetchOrders:async()=>[order],setLeverage:async()=>{},listenOHLCV:(_,cb)=>{callback=cb;return()=>{};}};
 const post=payload=>posts.push(structuredClone(payload));
 const call=new context.Handlers.CallMethodHandler('account','bybit',exchange,{postMessage:post},{},{});
 for(const [method,args] of [['fetchOHLCV',[{symbol:'BTC',interval:'1m'}]],['fetchOrders',[null]],['setLeverage',['BTC',3]],['notAMethod',[]]]){
  await call.handleCallMethod(method,method,args);const sent=posts.pop();
  if(method==='fetchOrders'){assert(Object.hasOwn(sent.data.result[0],'timeInForce'));assert.equal(sent.data.result[0].timeInForce,undefined);assert(!Object.hasOwn(sent.data.result[0],'parentId'));}
  if(method==='setLeverage')assert(Object.hasOwn(sent.data,'result')&&sent.data.result===undefined);
  if(method==='notAMethod')assert(!Object.hasOwn(sent.data,'result'));
  records.push({method,expected:observe(sent)});
 }
 const handler=new context.Handlers.OHLCVSubscriptionsHandler('account','bybit',exchange,{postMessage:post},{},{});
 handler.handle({op:'subscribeOHLCV',data:{symbol:'BTC',interval:'1m',subId:'a',tag:'metadata'}});
 callback(candles[0]);assert.equal(posts.length,1,'WORKER-DATA actual source one candle post');
 records.push({method:'listenOHLCV',expected:observe(posts.pop())});
 for(const method of ['fetchOrders','notAMethod']) {
  const attempts=[],failure=Object.assign(new Error('native post rejected'),{name:'TypeError',stack:'native stack',code:42});
  const rejected=new context.Handlers.CallMethodHandler('account','bybit',exchange,{postMessage(payload){attempts.push(structuredClone(payload));if(attempts.length===1)throw failure;}},{},{});
  await rejected.handleCallMethod('failed-post',method,[null]);
  assert.equal(attempts.length,2,'WORKER-DATA actual source post rejection catch');
  records.push({method:'post-rejected:'+method,expected:observe(attempts[1])});
 }
 assert.equal(records.length,7,'WORKER-DATA actual source output census');
 process.stdout.write(JSON.stringify({runtime:{node:process.version,v8:process.versions.v8,arch:process.arch},records}));
})().catch(e=>{console.error(e);process.exitCode=1;});
