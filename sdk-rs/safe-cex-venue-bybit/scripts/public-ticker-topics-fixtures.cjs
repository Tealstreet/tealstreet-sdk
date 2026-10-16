const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH),root=path.resolve(__dirname,'../../..');
const chunk=require(require.resolve('lodash/chunk',{paths:[root,path.resolve(process.env.SAFE_CEX_TYPESCRIPT_PATH,'../..')]}));
const methods=['getCurrentTickerTopics','getDesiredTickerTopics','syncTickerTopics','handleActiveTickerSymbolsChanged','onMarketsChanged'];
function subject(family){
 const file=path.join(root,'packages/safe-cex/src/exchanges/bybit',family==='inverse'?'bybitinverse.ws-public.ts':'bybitspot.ws-public.ts');
 const ast=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
 const name=family==='inverse'?'BybitInversePublicWebsocket':'BybitPublicWebsocket';
 const cls=ast.statements.find(s=>ts.isClassDeclaration(s)&&s.name.text===name);assert(cls);
 const body=methods.map(name=>{const matches=cls.members.filter(m=>m.name?.getText(ast)===name);assert.equal(matches.length,1,name);return matches[0].getText(ast)}).join('\n');
 const context=vm.createContext({chunk,sleep:()=>Promise.resolve()});
 vm.runInContext(ts.transpileModule(`class Subject {${body}} globalThis.subject=new Subject()`,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);
 return {api:context.subject,context};
}
const error=phase=>({name:`${phase}Error`,message:`${phase} failure`,code:37});
const observe=x=>x===undefined?{undefined:true}:x;
function setup(family,input){
 const {api,context}=subject(family),events=[],frames=[],errors=Object.fromEntries(['current','symbols','setter','send','sleep'].map(p=>[p,error(p)]));
 let current=Object.hasOwn(input,'current')?structuredClone(input.current):undefined;
 const original=current;let received,sendCount=0,sleepCount=0,disposed=!!input.disposed,connected=!!input.connected;
 api.topics={};
 Object.defineProperty(api.topics,'tickers',{get(){events.push(['current']);if(input.phase==='current')throw errors.current;return current},set(value){events.push(['setter',structuredClone(value)]);received=value;if(input.phase==='setterBefore')throw errors.setter;current=value;if(input.phase==='setterAfter')throw errors.setter;}});
 api.parent={getTickerSymbolsForPublicStream(){events.push(['symbols']);if(input.phase==='symbols')throw errors.symbols;if(input.mutateOriginal&&Array.isArray(original))original.push('tickers.LATE');if(input.disposeOnSymbols)disposed=true;if(input.connectOnSymbols)connected=true;return input.symbols||[];}};
 Object.defineProperty(api,'isDisposed',{get(){events.push(['disposed']);return disposed;}});
 Object.defineProperty(api,'isConnected',{get(){events.push(['connected']);return connected;}});
 Object.defineProperty(api,'ws',{get(){events.push(['ws']);return input.hasWs===false?undefined:{};}});
 api.send=frame=>{sendCount++;frames.push(frame);events.push(['send',JSON.parse(frame)]);if(sendCount===input.sendFailAt)throw errors.send;};
 context.sleep=async delay=>{sleepCount++;events.push(['sleep',delay]);await Promise.resolve();if(input.disposeOnSleep)disposed=true;if(sleepCount===input.sleepFailAt)throw errors.sleep;};
 const reject=e=>({error:e.name,message:e.message,code:e.code,origin:Object.keys(errors).find(p=>errors[p]===e)||null});
 return {api,events,frames,original,reject,state:()=>({current:observe(current),original:observe(original),received:observe(received),disposed,connected})};
}
async function main(){
 const current=[],desired=[],sync=[],active=[],markets=[];
 const currentInputs=[{}, {current:null},{current:false},{current:0},{current:''},{current:'tickers.OLD'},{current:[]},{current:['tickers.A','tickers.A']},{current:['tickers.QUOTE"','tickers.λ\n']},{current:[],phase:'current'}];
 const desiredInputs=[{symbols:[]},{symbols:['BTCUSD']},{symbols:['A','A']},{symbols:['tickers.A']},{symbols:['']},{symbols:['λ','QUOTE"','LINE\n']},{symbols:Array.from({length:11},(_,i)=>`S${i}`)},{symbols:[],phase:'symbols'}];
 const syncInputs=[{}, {current:'tickers.A',symbols:['B']},{current:['tickers.A','tickers.A'],symbols:['A','B','B']},{current:[],symbols:[]},{current:'',symbols:['']},{current:['tickers.A'],phase:'current'},{current:['tickers.A'],phase:'symbols'},{current:['tickers.A'],symbols:['B'],phase:'setterBefore'},{current:['tickers.A'],symbols:['B'],phase:'setterAfter'},{current:['tickers.A'],symbols:['B'],mutateOriginal:true}];
 const activeInputs=[];
 for(const count of [0,1,9,10,11,21])for(const connected of [false,true])activeInputs.push({current:['tickers.OLD'],symbols:Array.from({length:count},(_,i)=>`S${i}`),connected});
 activeInputs.push({current:['tickers.A','tickers.A'],symbols:['A','B','B'],connected:true},{current:'tickers.A',symbols:['A','B'],connected:true},{current:['tickers.OLD'],symbols:['NEW'],disposed:true,connected:true});
 for(const phase of ['current','symbols','setterBefore','setterAfter'])activeInputs.push({current:['tickers.OLD'],symbols:['NEW'],connected:true,phase});
 activeInputs.push({current:['tickers.OLD'],symbols:Array.from({length:21},(_,i)=>`S${i}`),connected:true,sendFailAt:1},{current:['tickers.OLD'],symbols:Array.from({length:21},(_,i)=>`S${i}`),connected:true,sendFailAt:2},{current:['tickers.A'],symbols:['B'],connected:false,connectOnSymbols:true},{current:['tickers.A'],symbols:['B'],connected:true,disposeOnSymbols:true},{current:['tickers.A'],symbols:['B'],connected:true,mutateOriginal:true},{current:['tickers.A','tickers.B'],symbols:['A'],connected:true});
 const marketInputs=[];
 for(const count of [0,1,9,10,11,21])marketInputs.push({current:['tickers.OLD'],added:Array.from({length:count},(_,i)=>`S${i}`)});
 marketInputs.push({current:['tickers.A','tickers.A'],added:['A','B','B']},{current:'tickers.OLD',added:['QUOTE"','λ','LINE\n']},{current:['tickers.OLD'],added:['NEW'],disposed:true},{current:['tickers.OLD'],added:['NEW'],hasWs:false});
 for(const phase of ['current','setterBefore','setterAfter'])marketInputs.push({current:['tickers.OLD'],added:['NEW'],phase});
 for(const key of ['sendFailAt','sleepFailAt'])for(const index of [1,2])marketInputs.push({current:['tickers.OLD'],added:Array.from({length:21},(_,i)=>`S${i}`),[key]:index});
 marketInputs.push({current:['tickers.OLD'],added:Array.from({length:11},(_,i)=>`S${i}`),disposeOnSleep:true});
 assert.equal(currentInputs.length,10);assert.equal(desiredInputs.length,8);assert.equal(syncInputs.length,10);assert.equal(activeInputs.length,25);assert.equal(marketInputs.length,18);
 for(const family of ['inverse','spot']){
  for(const input of currentInputs){const s=setup(family,input);let result;try{const value=s.api.getCurrentTickerTopics(),same=value===s.original;value.push('MUTATED');result={value, same};}catch(e){result=s.reject(e)}current.push({family,input,result,events:s.events,...s.state()});}
  for(const input of desiredInputs){const s=setup(family,input);let result;try{const value=s.api.getDesiredTickerTopics();value.push('MUTATED');result={value};}catch(e){result=s.reject(e)}desired.push({family,input,result,symbols:input.symbols,events:s.events,...s.state()});}
  for(const input of syncInputs){const s=setup(family,input);let result;try{const value=s.api.syncTickerTopics(),state=s.state(),sameCurrent=value===state.current,sameReceived=value===state.received;value.push('MUTATED');result={value,sameCurrent,sameReceived};}catch(e){result=s.reject(e)}sync.push({family,input,result,events:s.events,...s.state()});}
  for(const input of activeInputs){const s=setup(family,input);let result;try{s.api.handleActiveTickerSymbolsChanged();result={ok:true}}catch(e){result=s.reject(e)}active.push({family,input,result,events:s.events,frames:s.frames,...s.state()});}
  for(const input of marketInputs){const s=setup(family,input);let result;try{await s.api.onMarketsChanged({added:input.added.map(symbol=>({symbol,quote:'USD'}))});result={ok:true}}catch(e){result=s.reject(e)}markets.push({family,input,result,events:s.events,frames:s.frames,...s.state()});}
 }
 assert.equal(current.length,20);assert.equal(desired.length,16);assert.equal(sync.length,20);assert.equal(active.length,50);assert.equal(markets.length,36);
 process.stdout.write(JSON.stringify({current,desired,sync,active,markets}));
}
main().catch(e=>{console.error(e);process.exitCode=1});
