const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const runtimePaths={paths:[process.env.SAFE_CEX_TYPESCRIPT_PATH||__dirname]};
const runtime=process.argv.includes('--missing-runtime-control')?'safe-cex-missing-lodash-runtime/flatten':'lodash/flatten';
const flatten=require(require.resolve(runtime,runtimePaths));
assert.equal(require(require.resolve('lodash/package.json',runtimePaths)).version,'4.18.1','source lodash runtime pin');
const source=fs.readFileSync(path.resolve(__dirname,'../../../packages/safe-cex/src/exchanges/hyperliquid/hyperliquid.ws-public.ts'),'utf8');
const ast=ts.createSourceFile('hyperliquid.ws-public.ts',source,ts.ScriptTarget.Latest,true);
const cls=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text==='HyperliquidPublicWebSocket');
const member=cls?.members.find(n=>n.name?.getText(ast)==='subscribeUserTopics');assert(member,'user topic inventory');
const out={};vm.runInNewContext(ts.transpileModule('class Subject {'+member.getText(ast)+'} out.Subject=Subject;',{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,{out,flatten});
const cases=[];
const describe=subscription=>Object.keys(subscription).map(key=>({key,kind:subscription[key]===undefined?'undefined':typeof subscription[key],...(subscription[key]===undefined?{}:{value:subscription[key]})}));
for(const keys of [[],['candle.ETH.1m','l2Book.BTC'],['unknown.BTC','candle.SOL.5m','l2Book.ETH.extra'],['l2Book','candle.BTC'],['2','1','candle.BTC.1m','candle.ETH.5m'],['candle.BTC.part.1m','l2Book.ETH.part'],['l2Book.BTC','candle.ETH.1m'],['candle..','l2Book.'],['candle.BTC.1m.first','candle.BTC.1m.second','l2Book.ETH.first','l2Book.ETH.second']]) {
 for(const failAt of [0,1,2]) {
  const s=new out.Subject();s.topics={};for(const key of keys)s.topics[key]=0;const effects=[],nativeEffects=[];
  s.sendSubscription=subscription=>{effects.push(subscription);nativeEffects.push(describe(subscription));if(effects.length===failAt)throw new Error('user topic send witness');return false};
  let error;try{s.subscribeUserTopics()}catch(e){error={name:e.name,message:e.message}}
  cases.push({keys,ownKeys:Object.keys(s.topics),failAt,effects,nativeEffects,...(error?{error}:{})});
 }
}
{
 const s=new out.Subject();s.topics={'candle.BTC.1m':1,'l2Book.ETH':2};const effects=[],nativeEffects=[];let entered=false;
 s.sendSubscription=subscription=>{effects.push(subscription);nativeEffects.push(describe(subscription));if(!entered){entered=true;delete s.topics['l2Book.ETH'];s.topics['candle.SOL.5m']=3;s.subscribeUserTopics()}return false};
 s.subscribeUserTopics();cases.push({kind:'snapshot-reentry',keys:['candle.BTC.1m','l2Book.ETH'],effects,nativeEffects,finalKeys:Object.keys(s.topics)});
}
for(const coin of ['é','😀','\ud800','\udfff']) {
 const s=new out.Subject();s.topics={[`candle.${coin}.1m`]:0};const effects=[],nativeEffects=[];
 s.sendSubscription=subscription=>{effects.push(subscription);nativeEffects.push(describe(subscription));return false};s.subscribeUserTopics();
 cases.push({kind:'string-boundary',serializedKeys:JSON.stringify(Object.keys(s.topics)),serializedEffects:JSON.stringify(effects),serializedNativeEffects:JSON.stringify(nativeEffects)});
}
process.stdout.write(JSON.stringify({captureArchitecture:process.arch,nodeVersion:process.version,nodeMajor:Number(process.versions.node.split('.')[0]),scope:'actual user-topic member, own-key topic dictionary and injected subscription effects',cases}));
