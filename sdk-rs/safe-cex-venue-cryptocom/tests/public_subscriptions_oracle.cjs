const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path');
assert(process.env.SAFE_CEX_TYPESCRIPT_PATH,'SUBSCRIPTIONS compiler path required');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const file=path.resolve(__dirname,'../../../packages/safe-cex/src/exchanges/cryptocom/cryptocom.ws-public.ts');
const ast=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
const klass=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text==='CryptocomPublicWebSocket');assert(klass);
const methods=['resubscribe','sendSubscriptionFrames','registerAndSubscribe','subscribeToOrderBook','subscribeToTicker','subscribeToTickers','subscribeToTrades','subscribeToCandlestick'];
const chunk=ast.statements.flatMap(n=>ts.isVariableStatement(n)?Array.from(n.declarationList.declarations):[]).find(n=>n.name.getText(ast)==='SUBSCRIBE_CHUNK_SIZE');
const chunkSize=new Function('return ('+chunk.initializer.getText(ast)+')')();assert.equal(chunkSize,100);
const factories={};
for(const method of methods){const matches=klass.members.filter(n=>n.name?.getText(ast)===method);assert.equal(matches.length,1);assert(ts.isArrowFunction(matches[0].initializer));const compiled=ts.transpileModule('return ('+matches[0].initializer.getText(ast)+');',{reportDiagnostics:true,compilerOptions:{target:ts.ScriptTarget.ES2022}});assert.equal(compiled.diagnostics.filter(d=>d.category===ts.DiagnosticCategory.Error).length,0);factories[method]=new Function('SUBSCRIBE_CHUNK_SIZE','virtualClock','"use strict";\n'+compiled.outputText);}
const number=v=>v==='NaN'?NaN:v==='Infinity'?Infinity:v==='neg0'?-0:v;
const string=v=>typeof v==='string'?v:String.fromCharCode(...v.units);
const value=v=>v?.undefined||v?.hole?undefined:string(v);
function array(spec){const out=new Array(spec.length);for(let i=0;i<spec.length;i++)if(!spec[i]?.hole)out[i]=value(spec[i]);return out;}
const units=s=>Array.from({length:s.length},(_,i)=>s.charCodeAt(i));
const bits=n=>{const b=Buffer.alloc(8);b.writeDoubleBE(n);return b.toString('hex');};
const observe=v=>v===undefined?{kind:'undefined'}:{kind:'string',units:units(v)};
const slots=a=>Array.from({length:a.length},(_,i)=>Object.hasOwn(a,i)?{kind:'value',value:observe(a[i])}:{kind:'hole'});
if(process.argv.includes('--opaque-map')){
 const events=[],payload=['opaque'];let registered=false;
 const receiver=Object.defineProperty({},'registerAndSubscribe',{get(){events.push('resolve-register');return function(channels){assert.strictEqual(this,receiver);assert.strictEqual(channels,payload);events.push('register');registered=true;return Promise.resolve('ignored');};}});
 const symbols=[];Object.defineProperty(symbols,'map',{get(){events.push('resolve-map');return function(callback){assert.strictEqual(this,symbols);events.push('map');return payload;};}});
 const fn=factories.subscribeToTickers.call(receiver,chunkSize,{});
 assert.equal(fn(symbols),undefined);assert.equal(registered,true);
 process.stdout.write(JSON.stringify({registered,events}));process.exit(0);
}
const fixtures=JSON.parse(fs.readFileSync(path.join(__dirname,'public_subscriptions_fixtures.json'),'utf8'));
assert.equal(fixtures.length,68,'SUBSCRIPTIONS independent case floor');
const jsonDescriptor=Object.getOwnPropertyDescriptor(JSON,'stringify'),originalJson=jsonDescriptor.value;
const rows=[];
for(const spec of fixtures){
 const input=spec.input?array(spec.input):Array.from({length:spec.count??0},(_,i)=>'channel-'+i);
 const receiver={subscriptions:new Set(array(spec.initial??[])),messageId:number(spec.initialId??1)};
 const error=Error('original '+spec.name),events=[],frames=[],helpers=[],clockIds=[];
 let prepares=0,clocks=0,sends=0;
 const retained=receiver.subscriptions;
 if(spec.arrayEffect?.startsWith('index-')){
  const i=spec.getterIndex,original=input[i];
  Object.defineProperty(input,String(i),{configurable:true,get(){events.push('array-get-'+i);if(spec.arrayEffect==='index-throw')throw error;if(spec.arrayEffect==='index-grow')input.push('ETH');if(spec.arrayEffect==='index-shrink')input.length=1;return original;}});
 }
 if(spec.arrayEffect==='inherited'){const prototype=Object.create(Array.prototype);Object.defineProperty(prototype,'0',{get(){events.push('array-get-0');return 'Inherited';}});Object.setPrototypeOf(input,prototype);}
 if(spec.arrayEffect==='iterator-throw')Object.defineProperty(input,Symbol.iterator,{get(){events.push('resolve-iterator');return function(){events.push('iterator');let i=0;return{next(){events.push('next'+i);if(i++===0)return{done:false,value:'CUSTOM'};throw error;}};};}});
 const fail=(phase,ordinal=1)=>{if(spec.errorPhase===phase&&(spec.failOn??1)===ordinal)throw error;};
 const clock={};Object.defineProperty(clock,'getCurrentTimeMs',{get(){events.push('resolve-clock');fail('clock-get',clocks+1);return function(){assert.strictEqual(this,clock);events.push('clock');clocks++;clockIds.push(bits(receiver.messageId));fail('clock',clocks);if(clocks===1){if(spec.clockGrow)input.push(...array(spec.clockGrow));if(spec.clockShrink)input.length=0;if(spec.clockSetId!==undefined)receiver.messageId=number(spec.clockSetId);if(spec.clockAddSubscription)receiver.subscriptions.add(spec.clockAddSubscription);}return spec.clockValue!==undefined?number(spec.clockValue):1700000000000+13*clocks;};}});
 const actual={};for(const name of methods)actual[name]=factories[name].call(receiver,chunkSize,clock);
 for(const name of methods)receiver[name]=actual[name];
 Object.defineProperty(receiver,'sendSubscriptionFrames',{get(){events.push('resolve-frames');fail('frames-get');if(spec.framesAdd)receiver.subscriptions.add(spec.framesAdd);if(spec.framesReplace)receiver.subscriptions=new Set(array(spec.framesReplace));return function(channels){assert.strictEqual(this,receiver);events.push('frames');helpers.push({kind:'frames',channels:slots(channels)});fail('frames-call');const result=actual.sendSubscriptionFrames(channels);return spec.promiseReturns?Promise.resolve('ignored'):result;};}});
 Object.defineProperty(receiver,'registerAndSubscribe',{get(){events.push('resolve-register');fail('register-get');if(spec.registerGrow)input.push(...array(spec.registerGrow));return function(channels){assert.strictEqual(this,receiver);events.push('register');helpers.push({kind:'register',channels:slots(channels)});fail('register-call');const result=actual.registerAndSubscribe(channels);return spec.promiseReturns?Promise.resolve('ignored'):result;};}});
 Object.defineProperty(receiver,'send',{get(){prepares++;events.push('resolve-send');fail('send-get',prepares);if(spec.prepareShrink&&prepares===1)input.length=0;return function(wire){assert.strictEqual(this,receiver);events.push('send');sends++;fail('send',sends);if(spec.sendReenter&&sends===1)receiver.subscribeToTicker('REENTER');return spec.promiseReturns?Promise.resolve('ignored'):{ignored:true};};}});
 Object.defineProperty(JSON,'stringify',{configurable:true,get(){events.push('resolve-json');fail('json-get',prepares);return function(frame){events.push('stringify');frames.push({id:bits(frame.id),method:units(frame.method),channels:slots(frame.params.channels),nonce:bits(frame.nonce)});fail('stringify',prepares);return originalJson.call(JSON,frame);};}});
 let result;
 try{
  const symbol=spec.symbol===undefined?'BTC':string(spec.symbol);
  let out;
  switch(spec.op){
   case'frames':out=actual.sendSubscriptionFrames(input);break;
   case'register':out=actual.registerAndSubscribe(input);break;
   case'replay':out=actual.resubscribe();break;
   case'book':out=spec.depth===undefined?actual.subscribeToOrderBook(symbol):actual.subscribeToOrderBook(symbol,number(spec.depth));break;
   case'ticker':out=actual.subscribeToTicker(symbol);break;
   case'tickers':out=actual.subscribeToTickers(input);break;
   case'trades':out=actual.subscribeToTrades(symbol);break;
   case'candle':out=actual.subscribeToCandlestick(symbol,string(spec.interval));break;
   default:throw Error('unknown fixture op');
  }
  assert.equal(out,undefined,'SUBSCRIPTIONS native void '+spec.name);result='void';
 }catch(caught){assert.strictEqual(caught,error,'SUBSCRIPTIONS original error '+spec.name);result='original';}
 finally{Object.defineProperty(JSON,'stringify',jsonDescriptor);}
 assert.equal(frames.length,spec.expectedFrames,'SUBSCRIPTIONS independent source frame floor '+spec.name);
 rows.push({name:spec.name,result,subscriptions:Array.from(receiver.subscriptions,observe),retainedSubscriptions:Array.from(retained,observe),messageId:bits(receiver.messageId),frames,helpers,clockIds,events});
}
assert.equal(rows.length,68,'SUBSCRIPTIONS independent result floor');
process.stdout.write(originalJson(rows));
