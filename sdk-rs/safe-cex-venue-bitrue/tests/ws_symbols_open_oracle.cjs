const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path');
const {ts,root,load}=require('./configuration.cjs');
const INTERVAL=load(path.join(root,'packages/safe-cex/src/exchanges/bitrue/bitrue.types.ts')).INTERVAL;
const units=text=>Array.from({length:text.length},(_,i)=>text.charCodeAt(i));
function initializer(file,klass,name,receiver,deps={}){
 const filename=path.join(root,'packages/safe-cex/src/exchanges/bitrue/'+file);
 const source=ts.createSourceFile(filename,fs.readFileSync(filename,'utf8'),ts.ScriptTarget.Latest,true);
 const classes=source.statements.filter(n=>ts.isClassDeclaration(n)&&n.name?.text===klass);assert.equal(classes.length,1);
 const members=classes[0].members.filter(n=>n.name?.getText(source)===name);assert.equal(members.length,1);assert(ts.isArrowFunction(members[0].initializer));
 const compiled=ts.transpileModule('return ('+members[0].initializer.getText(source)+');',{reportDiagnostics:true,compilerOptions:{target:ts.ScriptTarget.ES2022}});
 assert.equal(compiled.diagnostics.filter(d=>d.category===ts.DiagnosticCategory.Error).length,0);
 return new Function(...Object.keys(deps),'"use strict";\n'+compiled.outputText).call(receiver,...Object.values(deps));
}
const publicFn=(name,receiver,deps)=>initializer('bitrue.ws-public.ts','BitruePublicWebsocket',name,receiver,deps);
const rows=[];
const symbols=['','BTCUSDT','E-BTC-USDT','e_BTC/USDT','_E-BTC-USDT','E/BTC-USDT','EE-BTC-USDT','E--BTC__/_USDT','E_ΟΣ','E_İI','E_Σ','E_ΟΣΑ','\ud800E-\udfff/BTC','E-\ud800_/\udfff','E-','e_','E-BTC-USDT\n','E-БТК-USD','E-K-İ-ẞ','E-BTC'];
assert.equal(symbols.length,20,'WS-SYMBOLS independent converter floor');
const lower=String.prototype.toLowerCase;
const converter=publicFn('convertToWsSymbol',undefined);
for(let index=0;index<symbols.length;index++){
 const input=symbols[index],events=[];let normalized,lowered,thrown;
 String.prototype.toLowerCase=function(){'use strict';normalized=units(String(this));events.push('lower');if(index===19){thrown=Error('lower');throw thrown;}const value=lower.call(this);lowered=units(value);return value;};
 let result;
 try{result={kind:'string',units:units(converter(input))};}
 catch(error){assert.strictEqual(error,thrown);result={kind:'original'};}
 finally{String.prototype.toLowerCase=lower;}
 rows.push({group:'convert',id:'symbol'+index,input:units(input),normalized,lowered:lowered??null,error:index===19,events,result});
}
const topicSpecs=[...['1m','3m','5m','15m','30m','1h','2h','4h','6h','12h','1d','1w','unknown','constructor','toString','__proto__'].map(key=>[key,key]),['symbol-surrogate','1m'],['converter-number','1m'],['convert-get-throw','1m'],['convert-call-throw','1m'],['convert-coercion-throw','1m'],['interval-key-throw',null],['interval-get-throw','1m'],['interval-coercion-throw','1m']];
assert.equal(topicSpecs.length,24,'WS-SYMBOLS independent topic floor');
for(const[id,rawKey]of topicSpecs){
 const input=id==='symbol-surrogate'?'E-\ud800_\udfff':'E-BTC-USDT',events=[],convertEvents=[],intervalEvents=[];let converted,term,thrown,conversionError=false,intervalError=false;
 let interval=rawKey;if(id==='interval-key-throw')interval={[Symbol.toPrimitive](){thrown=Error('key');throw thrown;}};
 const receiver=Object.defineProperty({},'convertToWsSymbol',{get(){events.push('resolve-convert');convertEvents.push('resolve-convert');if(id==='convert-get-throw'){thrown=Error('resolve');conversionError=true;throw thrown;}return function(symbol){
  assert.strictEqual(this,receiver);assert.strictEqual(symbol,input);events.push('convert');convertEvents.push('convert');
  try{
   if(id==='convert-call-throw')throw Error('convert');
   const result=id==='converter-number'?17.5:id==='convert-coercion-throw'?Symbol('converted'):publicFn('convertToWsSymbol',undefined)(symbol);
   // Instrument the source call-plus-template conversion boundary once.
   const text=`${result}`;converted=units(text);return text;
  }catch(error){thrown=error;conversionError=true;throw error;}
 };}});
 const table=new Proxy(INTERVAL,{get(target,key){events.push('interval');intervalEvents.push('interval');try{if(id==='interval-get-throw')throw Error('interval');const value=id==='interval-coercion-throw'?Symbol('term'):Reflect.get(target,key);const text=`${value}`;term=units(text);return text;}catch(error){thrown=error;intervalError=true;throw error;}}});
 let result;
 try{result={kind:'string',units:units(publicFn('formatKlineTopic',receiver,{INTERVAL:table})(input,interval))};}
 catch(error){assert.strictEqual(error,thrown);if(!conversionError)intervalError=true;result={kind:'original'};}
 rows.push({group:'topic',id,input:units(input),converted:converted??null,term:term??null,conversionError,intervalError,convertEvents,intervalEvents,events,result});
}
const openSpecs=[['live',false],['disposed',true],['truthy-string','yes'],['falsy-zero',0],['falsy-null',null],['guard-throw',false],['subscribe-get-throw',false],['subscribe-throw',false],['ping-get-throw',false],['ping-throw',false],['dispose-during-subscribe',false],['promise',false]];
assert.equal(openSpecs.length,12,'WS-SYMBOLS independent private-open floor');
for(const[id,initial]of openSpecs){
 const events=[];let disposed=initial,thrown;
 const receiver={};
 Object.defineProperty(receiver,'isDisposed',{get(){events.push('guard');if(id==='guard-throw'){thrown=Error('guard');throw thrown;}return disposed;}});
 for(const name of['subscribe','ping'])Object.defineProperty(receiver,name,{get(){events.push('resolve-'+name);if(id===name+'-get-throw'){thrown=Error('getter');throw thrown;}return function(){assert.strictEqual(this,receiver);events.push(name);if(id===name+'-throw'){thrown=Error('call');throw thrown;}if(id==='dispose-during-subscribe'&&name==='subscribe')disposed=true;return id==='promise'?Promise.resolve('ignored'):{ignored:true};};}});
 let result;
 try{const value=initializer('bitrue.ws-private.ts','BitruePrivateWebsocket','onOpen',receiver)();assert.equal(value,undefined);result={kind:'void'};}
 catch(error){assert.strictEqual(error,thrown);result={kind:'original'};}
 rows.push({group:'open',id,guard:!!initial,events,result});
}
assert.equal(rows.length,56,'WS-SYMBOLS independent total floor');
process.stdout.write(JSON.stringify(rows));
