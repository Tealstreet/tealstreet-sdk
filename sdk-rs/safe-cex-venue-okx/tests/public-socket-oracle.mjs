import fs from 'node:fs';import vm from 'node:vm';import assert from 'node:assert/strict';
import {ts} from '../../safe-cex-store/tests/source_loader.mjs';import {loadSource} from './source-loader.mjs';
const url=new URL('../../../packages/safe-cex/src/exchanges/okx/okx.ws-public.ts',import.meta.url);
let source=fs.readFileSync(url,'utf8');if(process.argv.includes('--mutate-gap-guard'))source=source.replace('previousSeqId !== expectedPreviousSeqId','false');
if(process.argv.includes('--mutate-prototype'))source=source.replace('messageHandlers: MessageHandlers = {','messageHandlers: MessageHandlers = Object.assign(Object.create(null), {').replace("'funding-rate': [(d: Data) => this.handleFundingRateEvents(d)],\n  };","'funding-rate': [(d: Data) => this.handleFundingRateEvents(d)],\n  });");
const ast=ts.createSourceFile(url.pathname,source,ts.ScriptTarget.Latest,true),owner=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text==='OKXPublicWebsocket');
const names=['messageHandlers','onMessage','dispatchMessageHandler','acceptOrderBookSequence','recoverOrderBook'];const members=names.map(name=>{const member=owner.members.find(n=>n.name?.getText(ast)===name);assert.ok(member,name);return member.getText(ast);});
const {bigIntAsStringJsonParse}=loadSource(new URL('../../../packages/json-parse/src/jsonParse.ts',import.meta.url));
const module={exports:{}};const code=ts.transpileModule('export class Selected {'+members.join('\n')+'}',{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
vm.runInThisContext('(function(module,exports,bigIntAsStringJsonParse){'+code+'\n})')(module,module.exports,bigIntAsStringJsonParse);
const baseChannels=['tickers','mark-price','index-tickers','open-interest','funding-rate'];
const cases=[];
function run(input){const selected=new module.exports.Selected(),calls=[],sent=[],results=[],stats={};let tracked=0;
 selected.orderBookSequenceByHandler=new Map(input.sequences??[]);selected.recoveringOrderBooks=new Set(input.recovering??[]);selected.topics=input.topics??{};selected.isConnected=input.connected??true;selected.isDisposed=input.disposed??false;
 selected.trackLastReceived=()=>tracked++;selected.directFallback={onMessage:()=>{}};selected.handlePongEvent=()=>{stats.pong={count:1,had:false};};
 selected.recordMessageHandlerStats=key=>{const list=selected.messageHandlers[key];stats[key]={count:list?.length??1,had:!!list?.length};};
 selected.send=text=>{const index=sent.length;sent.push(text);if(index===input.failSendAt)throw Error('send failure');if(input.reenter&&index===0)selected.onMessage({data:input.reenter});};
 for(const key of [...baseChannels,...(input.handlers??[])])selected.messageHandlers[key]=Array.from({length:input.count??1},(_,index)=>json=>{calls.push({key,index,data:JSON.stringify(json),stats:stats[key]});if(input.removeKey&&index===0)delete selected.messageHandlers[key];if(input.throwFirst&&index===0)throw new TypeError('callback failed');});
 for(const frame of input.frames){try{selected.onMessage({data:frame});results.push(null);}catch(error){results.push({name:error.name,message:error.message});}}
 const result={input,calls,sent,results,stats,tracked,sequences:[...selected.orderBookSequenceByHandler],recovering:[...selected.recoveringOrderBooks]};
 if(input.prototype){
  const inherited=Object.prototype[input.prototype];
  const kind=typeof inherited, arity=kind==='function'?inherited.length:null;
  result.sourcePrototype={kind,arity,results,stats};
  if(kind==='function'&&arity>0){
   assert.ok(results[0]?.message.includes('callbacks is not iterable'),'source positive-arity callback defect must stay pinned');
   assert.equal(stats[input.prototype].count,arity,'source handler stats must retain inherited arity');
   assert.equal(stats[input.prototype].had,true,'positive arity must pass the source length guard');
   result.stats={};
  }else{assert.deepEqual(results,[null],'zero-arity/prototype object must return at length guard');assert.deepEqual(stats,{});}
 }

 cases.push(result);return result;}
const frame=(action,seqId,prevSeqId,channel='books',instId='BTC')=>JSON.stringify({arg:{channel,instId},action,data:[{seqId,prevSeqId}]});
const topic={channel:'books',instId:'BTC'};
const gap=run({handlers:['books.BTC','books'],topics:{'books.BTC':[topic]},frames:[frame('snapshot','10'),frame('update','11','wrong'),frame('update','12','11'),frame('snapshot','20'),frame('update','21','20')]});assert.equal(gap.calls.length,8,'sequence gap must suppress matched book callbacks until a snapshot');assert.equal(gap.sent.length,2,'gap must unsubscribe then subscribe once');
for(const seqId of [undefined,null,'',0,1,'90071992547409931',[],['10'],{}, {toString:null}])for(const prevSeqId of [undefined,null,'',0,1,'10','wrong',{}, {toString:null}])run({handlers:['books.BTC'],topics:{'books.BTC':[topic]},sequences:[['books.BTC','10']],frames:[frame('update',seqId,prevSeqId)]});
for(const action of ['snapshot','update',undefined])for(const recovering of [[],['books.BTC']])run({handlers:['books.BTC'],recovering,frames:[frame(action,'10','9')]});
for(const connected of [true,false])for(const disposed of [true,false])for(const failSendAt of [undefined,0,1])run({handlers:['books.BTC'],topics:{'books.BTC':[topic]},connected,disposed,failSendAt,frames:[frame('update','2','1')]});
for(const channel of ['tickers','mark-price','index-tickers','open-interest','funding-rate','unknown','',0,null])run({handlers:['tickers.BTC'],frames:[JSON.stringify({arg:{channel,instId:'BTC'},data:[{price:1e-7}]})]});
for(const frame of ['pong','{','null','[]','42','\uFEFF{}','{"event":"subscribe","arg":{"channel":"tickers"}}','{"arg":{"channel":"tickers","instId":"BTC"},"data":[{"seqId":90071992547409931}]}'])run({handlers:['tickers.BTC'],frames:[frame]});
for(const prototype of Object.getOwnPropertyNames(Object.prototype))run({prototype,frames:[JSON.stringify({arg:{channel:prototype},data:[]})]});
for(const throwFirst of [false,true])for(const removeKey of [false,true])run({handlers:['tickers.BTC'],count:3,throwFirst,removeKey,frames:[JSON.stringify({arg:{channel:'tickers',instId:'BTC'},data:[]})]});
run({handlers:['books.BTC'],topics:{'books.BTC':[topic]},reenter:frame('update','2','1'),frames:[frame('update','2','1')]});
const nonfinite=run({handlers:['books.BTC'],topics:{'books.BTC':[topic]},frames:['{"arg":{"channel":"books","instId":"BTC"},"action":"snapshot","data":[{"seqId":1e400}]}']});
assert.deepEqual(nonfinite.sequences,[['books.BTC','Infinity']],'source uses String(Infinity) before JSON projection');
nonfinite.sourceNonfinite={calls:nonfinite.calls,sent:nonfinite.sent,results:nonfinite.results,stats:nonfinite.stats,sequences:nonfinite.sequences,recovering:nonfinite.recovering};
assert.equal(cases.length,144);
assert.equal(cases.filter(c=>c.sourcePrototype?.arity>0).length,8);
assert.equal(cases.filter(c=>c.sourcePrototype?.arity===0).length,3);
assert.equal(cases.filter(c=>c.sourcePrototype?.kind==='object').length,1);process.stdout.write(JSON.stringify(cases));
