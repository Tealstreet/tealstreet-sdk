const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const source=fs.readFileSync(path.resolve(__dirname,'../../../packages/safe-cex/src/exchanges/hyperliquid/hyperliquid.ws-public.ts'),'utf8');
const ast=ts.createSourceFile('hyperliquid.ws-public.ts',source,ts.ScriptTarget.Latest,true);
const cls=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text==='HyperliquidPublicWebSocket');
assert(cls,'public class inventory');
const names=['subscribePublicTopics','sendSubscription','sendUnsubscription'];
const members=cls.members.filter(n=>names.includes(n.name?.getText(ast)));
assert.equal(members.length,3,'public topic member inventory');
const out={};vm.runInNewContext(ts.transpileModule('class Subject {'+members.map(n=>n.getText(ast)).join('\n')+'} out.Subject=Subject;',{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,{out});
const cases=[];
for(const failure of ['none','send-1','send-2','user']) {
 const s=new out.Subject();s.activeSubscriptions=new Set([JSON.stringify({type:"allMids"})]);const effects=[];let sends=0;
 s.sendWithDiagnostics=message=>{sends++;effects.push({kind:'send',message,active:[...s.activeSubscriptions]});if(failure===`send-${sends}`)throw new Error(failure)};
 s.subscribeUserTopics=()=>{effects.push({kind:'user',active:[...s.activeSubscriptions]});if(failure==='user')throw new Error(failure)};
 let error;try{s.subscribePublicTopics();if(failure==='none')s.subscribePublicTopics()}catch(e){error={name:e.name,message:e.message}}
 cases.push({failure,effects,...(error?{error}:{}),active:[...s.activeSubscriptions]});
}
{
 const s=new out.Subject();s.activeSubscriptions=new Set();const effects=[];let entered=false;
 const nested={type:'bbo',coin:'REENTRY'};
 s.sendWithDiagnostics=message=>{effects.push({kind:'send',message,active:[...s.activeSubscriptions]});if(!entered){entered=true;s.sendSubscription(nested)}};
 s.subscribeUserTopics=()=>{effects.push({kind:'user',active:[...s.activeSubscriptions]});s.sendUnsubscription(nested)};
 s.subscribePublicTopics();cases.push({failure:'reentry',effects,active:[...s.activeSubscriptions]});
}
process.stdout.write(JSON.stringify({captureArchitecture:process.arch,nodeVersion:process.version,scope:'actual public topic/subscription bodies; injected diagnostics sender and user-topic effects; no lifecycle claim',cases}));
