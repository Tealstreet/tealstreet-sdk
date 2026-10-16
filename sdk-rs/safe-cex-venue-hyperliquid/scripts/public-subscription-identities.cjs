const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm'), assert = require('node:assert/strict');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const source = fs.readFileSync(path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/hyperliquid/hyperliquid.ws-public.ts'), 'utf8');
const ast = ts.createSourceFile('hyperliquid.ws-public.ts', source, ts.ScriptTarget.Latest, true);
const cls = ast.statements.find(n => ts.isClassDeclaration(n) && n.name?.text === 'HyperliquidPublicWebSocket');
assert(cls && !cls.heritageClauses?.some(h => h.types.length !== 1), 'unexpected public socket class heritage');
const names = ['sendSubscription', 'sendUnsubscription'];
const members = cls.members.filter(n => names.includes(n.name?.getText(ast)));
assert.equal(members.length, 2, 'subscription member inventory');
const out = {};
vm.runInNewContext(ts.transpileModule('class Subject {' + members.map(n => n.getText(ast)).join('\n') + '} out.Subject=Subject;', {compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText, {out});
const cases = [];
const bits = require('./number-bits.cjs');
const nativeDepth = value => !Object.hasOwn(value,'depth') ? {kind:'missing'} : value.depth === null ? {kind:'null'} : {kind:typeof value.depth,bits:bits(value.depth)};
for (const failure of [false, true]) {
 const s = new out.Subject(); s.activeSubscriptions = new Set(); const events = [];
 s.sendWithDiagnostics = message => { events.push({message, active:[...s.activeSubscriptions]}); if(failure) throw new Error('send witness'); };
 for (const [operation, subscription] of [['subscribe',{type:'candle',coin:'ETH',interval:'1m'}],['subscribe',{type:'candle',coin:'ETH',interval:'1m'}],['unsubscribe',{type:'candle',coin:'ETH',interval:'1m'}],['subscribe',{type:'bbo',coin:'BTC'}],['unsubscribe',{type:'bbo',coin:'ABSENT'}]]) {
  let result, error; try { result = operation === 'subscribe' ? s.sendSubscription(subscription) : s.sendUnsubscription(subscription); } catch(e) { error = {name:e.name,message:e.message}; }
  cases.push({failure,operation,subscription,...(result===undefined?{}:{result}),...(error?{error}:{}),active:[...s.activeSubscriptions],events:events.slice()});
 }
}
for (const subscriptions of [
 [{type:'bbo',coin:'BTC'},{coin:'BTC',type:'bbo'}],
 [{type:'bbo',coin:'BTC',depth:-0},{type:'bbo',coin:'BTC',depth:0}],
 [{type:'bbo',coin:'BTC',depth:NaN},{type:'bbo',coin:'BTC',depth:null}],
 [{type:'bbo',coin:'BTC',depth:Infinity},{type:'bbo',coin:'BTC',depth:null}],
 [{'2':'second','1':'first',type:'bbo'},{'1':'first','2':'second',type:'bbo'}],
]) {
 const s=new out.Subject();s.activeSubscriptions=new Set();const events=[];s.sendWithDiagnostics=message=>events.push(message);
 const results=subscriptions.map(subscription=>s.sendSubscription(subscription));
 cases.push({kind:'key-identity',nativeDepths:subscriptions.map(nativeDepth),inputKeys:subscriptions.map(JSON.stringify),results,events,active:[...s.activeSubscriptions]});
}
{
 const s=new out.Subject();s.activeSubscriptions=new Set();const events=[];let entered=false;
 const subscription={type:'candle',coin:'SOL',interval:'5m'};
 s.sendWithDiagnostics=message=>{events.push(message);if(!entered){entered=true;s.sendUnsubscription(subscription);s.sendSubscription(subscription);}};
 const result=s.sendSubscription(subscription);
 assert.equal(events.length,3,'reentry must expose committed subscription state');
 cases.push({kind:'reentry',result,events,active:[...s.activeSubscriptions]});
}
for(const coin of ['BTC','é','😀','\ud800','\udfff']) {
 const subscription={type:'bbo',coin};const s=new out.Subject();s.activeSubscriptions=new Set();const events=[];s.sendWithDiagnostics=message=>events.push(message);
 const result=s.sendSubscription(subscription);
 cases.push({kind:'string-boundary',serializedInput:JSON.stringify(subscription),result,events,active:[...s.activeSubscriptions]});
}
for(const throwOnSecond of [false,true]) {
 const s=new out.Subject();s.activeSubscriptions=new Set();const events=[];let evaluations=0;
 const subscription={type:'bbo',get coin(){evaluations++;if(throwOnSecond&&evaluations===2)throw new TypeError('second evaluation witness');return evaluations%2===1?'BTC':'ETH'}};
 s.sendWithDiagnostics=message=>events.push(message);
 const steps=[];
 for(let i=0;i<2;i++){let result,error;try{result=s.sendSubscription(subscription)}catch(e){error={name:e.name,message:e.message}}steps.push({...(result===undefined?{}:{result}),...(error?{error}:{}),evaluations,events:events.slice(),active:[...s.activeSubscriptions]});}
 cases.push({kind:'evaluation-count',throwOnSecond,steps});
}
process.stdout.write(JSON.stringify({scope:'actual source bodies, synchronous sender effects; no BaseWebSocket lifecycle claim',captureArchitecture:process.arch,nodeVersion:process.version,cases}));
