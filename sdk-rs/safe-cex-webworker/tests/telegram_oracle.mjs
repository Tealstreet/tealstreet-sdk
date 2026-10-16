import fs from 'node:fs';import vm from 'node:vm';import assert from 'node:assert/strict';import {createRequire} from 'node:module';
const require=createRequire(import.meta.url),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const path=new URL('../../../packages/safe-cex/src/webworker/TelgramWorker.ts',import.meta.url);
const ast=ts.createSourceFile(path.pathname,fs.readFileSync(path,'utf8'),ts.ScriptTarget.Latest,true);
const cls=ast.statements.find(ts.isClassDeclaration),ctor=cls.members.find(ts.isConstructorDeclaration);
assert.equal(ctor.parameters.length,2);assert.deepEqual(ctor.body.statements.map(x=>x.expression.left.name.text),['_getExchange','_token']);
const code=ast.statements.filter(x=>!ts.isImportDeclaration(x)).map(x=>x.getText(ast)).join('\n').replace(/\bexport /g,'');
const source=ts.transpileModule(code+'\nglobalThis.Worker=TelegramWorker;', {compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText;
const bytes=new DataView(new ArrayBuffer(8));function bits(x){bytes.setFloat64(0,x);return bytes.getBigUint64(0).toString(16).padStart(16,'0');}
const positions=[['BTC','long',-12.5],['ETH','short',-0],['NAN','long',NaN],['INF','short',Infinity],['SMALL','short',1e-7],['BIG','long',1e21]].map(([symbol,side,unrealizedPnl])=>({symbol,side,unrealizedPnl}));
const settle=async()=>{for(let i=0;i<8;i++)await Promise.resolve();};
function setup(mode='ok'){
 const effects=[],timers=[],queue=[];let pending,current,getCalls=0;
 const exchange=id=>({id,store:{positions},emitter:{emit:(event,data)=>effects.push(['log',id,event,data.message])}});
 const a=exchange('bybit'),b=exchange('binance');current=a;
 const getter=()=>{getCalls++;if(mode==='getter'&&getCalls===1){effects.push(['getter-error']);throw new Error('getter');}effects.push(['exchange',current.id]);return current;};
 const executor=mode==='none'?null:(ex,symbol,command)=>{effects.push(['exec',ex.id,symbol,command]);if(mode==='sync')throw new Error('sync');if(mode==='async')return Promise.reject(new Error('async'));return Promise.resolve();};
 const context=vm.createContext({Error,axios:{get:url=>{effects.push(['get',url]);const frame=queue.shift()||{messages:[]};if(frame.fail)return Promise.reject(new Error('get fail'));if(frame.pending)return new Promise(resolve=>{pending=messages=>resolve({data:{result:messages}});});return Promise.resolve({data:{result:frame.messages}});},post:(url,body)=>{effects.push(['post',url,bits(body.chat_id),body.text]);return Promise.resolve();}},PositionSide:{Long:'long'},getCommandExecutor:()=>executor,setTimeout:(fn,ms)=>{effects.push(['timer',ms]);timers.push(fn);return timers.length;}});
 vm.runInContext(source,context);
 const worker=new context.Worker(getter,'token:é');
 return {effects,timers,queue,worker,swap:()=>{current=b;},complete:x=>pending(x),getCalls:()=>getCalls};
}
const msg=(text,id=42)=>({update_id:id,message:{chat:{id:-7.5},text}});
const tokens=['bot:a','é🪙','',JSON.parse('"\\ud800"')];
const constructors=tokens.map(token=>{const context=vm.createContext({});vm.runInContext(source,context);let current={value:17},calls=0;const first=current,receivers=[],reads=[];function getter(){calls++;receivers.push(this);reads.push(this._token.length);return current;}const w=new context.Worker(getter,token);const a=w._exchange;current={value:42};const second=current,b=w._exchange;return {token:Array.from({length:token.length},(_,i)=>token.charCodeAt(i)),counter:bits(w._tickCounter),seen:w._seenMessages.size,callbackSame:w._getExchange===getter,firstSame:a===first,secondSame:b===second,receiverSame:receivers.every(x=>x===w),reads,calls};});
const inputs=[['positions','ok','/positions'],['positions-space','ok','/positions '],['unknown','ok','x'],['cli','ok','/cli BTC buy 0.01'],['usage-symbol','ok','/cli BTC'],['usage-empty','ok','/cli '],['unicode-space','ok','/cli \tETH\u00a0sell\ufeff2  '],['prefix-tab','ok','/cli\tBTC buy 1'],['nel','ok','/cli \u0085BTC buy'],['line-space','ok','/cli BTC\tbuy\n1'],['trailing','ok','/cli BTC buy '],['usage-other','ok','/cli /positions'],['sync','sync','/cli BTC buy 1'],['async-swap','async','/cli BTC buy 1'],['none','none','/cli BTC buy 1'],['getter','getter','/cli BTC buy 1'],['duplicate','ok','/positions'],['repeat','ok','/positions']];
const direct=[];
for(const [label,mode,text] of inputs){const s=setup(mode);const batch=label==='duplicate'?[msg(text),msg(text)]:[msg(text)];const promise=s.worker.processMessages(batch);const prefix=structuredClone(s.effects);if(label==='async-swap')s.swap();await promise;if(label==='repeat')await s.worker.processMessages(batch);await settle();direct.push({label,prefix,effects:s.effects,counter:bits(s.worker._tickCounter),seen:s.worker._seenMessages.has(42),getCalls:s.getCalls()});}
const s=setup(),trace=[];const queries=[1,42,43,44,45,100,NaN,0];
function snapshot(label){trace.push({label,effects:structuredClone(s.effects),counter:bits(s.worker._tickCounter),seen:queries.map(id=>s.worker._seenMessages.has(id))});}
for(let i=1;i<=9;i++){s.queue.push({messages:[msg('/positions',i===1?1:20+i),...(i===1?[msg('ignored',NaN),msg('ignored',-0)]:[])]});await s.worker.tick();await settle();snapshot('warm-'+i);}
s.queue.push({messages:[msg('/positions',1),msg('/positions',42),msg('/positions',42),msg('unknown',43),msg('/cli BTC buy 1',44)]});await s.worker.tick();await settle();snapshot('dispatch-ten');
s.queue.push({messages:[msg('/positions',42),msg('/positions',45),msg('/positions',NaN),msg('/positions',0)]});await s.worker.tick();await settle();snapshot('dispatch-eleven');
s.queue.push({fail:true});await assert.rejects(s.worker.tick(),/get fail/);snapshot('get-fail');
s.queue.push({pending:true});const waiting=s.worker.tick();snapshot('get-pending');s.complete([msg('/positions',100)]);await waiting;await settle();snapshot('get-settled');
const start=setup(),startRows=[];start.queue.push({messages:[msg('warm',1)]},{messages:[msg('warm',2)]});
const startSnap=label=>startRows.push({label,effects:structuredClone(start.effects),counter:bits(start.worker._tickCounter),seen:[1,2].map(id=>start.worker._seenMessages.has(id))});
start.worker.start();startSnap('start-prefix');await settle();startSnap('start-settled');start.timers.shift()();await settle();startSnap('timer-fired');
assert.equal(constructors.length,4);assert.equal(direct.length,18);assert.equal(trace.length,14);assert.equal(startRows.length,3);
// Source string witnesses for the chosen runtime String boundary; constructor remains opaque.
const lone=setup();await lone.worker.processMessages([msg(JSON.parse('"/cli \\ud800 buy 1"'))]);await settle();
process.stdout.write(JSON.stringify({constructors,direct,trace,startRows,loneSymbol:lone.effects.find(x=>x[0]==='exec')[2].charCodeAt(0)}));
