// Actual source members only; this oracle does not execute or claim the constructor.
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict'),crypto=require('node:crypto');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const file=path.resolve(__dirname,'../../../packages/safe-cex/src/exchanges/hyperliquid/hyperliquid.exchange.ts');
const raw=fs.readFileSync(file,'utf8'),ast=ts.createSourceFile(file,raw,ts.ScriptTarget.Latest,true);
const cls=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text==='HyperliquidExchange');
const names=['_placedLimitOrderTif','PLACED_TIF_CACHE_LIMIT','rememberLimitOrderTimeInForce','getPlacedLimitOrderTimeInForce','forgetLimitOrderTimeInForce','_cancelledOrders','CANCEL_SUPPRESSION_MAX_AGE_MS','isOrderCancelled','suppressCancelledOrder','unsuppressCancelledOrder','hasPrivateSessionIdentity','isPublicDataOnly'];
const members=cls.members.filter(n=>names.includes(n.name?.getText(ast)));assert.equal(members.length,names.length,'real source members must remain inventoried');
let code='class Subject {\n'+members.map(n=>n.getText(ast)).join('\n')+'\n}\nout.Subject=Subject;';
if(process.argv.includes('--control-age'))code=code.replace('> this.CANCEL_SUPPRESSION_MAX_AGE_MS','>= this.CANCEL_SUPPRESSION_MAX_AGE_MS');
if(process.argv.includes('--control-identity'))code=code.replace('this.options.vault || this.options.applicationId','this.options.key && this.options.secret');
let now=0,beforeNow;const out={};vm.runInNewContext(ts.transpileModule(code,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,{out,Date:{now:()=>{const hook=beforeNow;beforeNow=undefined;if(hook)hook();return now;}}});
const typed=process.argv.includes('--typed');
const cases=[];
for(const options of [{},{key:'signed',secret:'key'},{applicationId:'master'},{vault:'vault'},{vault:'',applicationId:'master'},{applicationId:''},{vault:'vault',publicDataOnly:true},{applicationId:'master',publicDataOnly:false},{publicDataOnly:false},{key:'signed',secret:'key',publicDataOnly:false}]) {
 const s=new out.Subject();s.options=options;cases.push({kind:'privacy',options,identity:s.hasPrivateSessionIdentity,public:s.isPublicDataOnly});
}
const s=new out.Subject();const ops=[['remember','first','GTC'],['remember','second','IOC'],['get','first'],['remember','first','FOK'],['forget','second'],['get','second']];
for(let i=0;i<501;i++)ops.push(['remember','k'+i,'GTC']);ops.push(['get','first'],['get','k0'],['get','k1'],['remember','k1','IOC'],['remember','last','FOK'],['get','k1'],['get','k2']);
for(let [op,id,tif] of ops) {
 if(typed&&tif)tif={GTC:'GoodTillCancel',IOC:'ImmediateOrCancel',FOK:'FillOrKill'}[tif];
 let value;
 if(op==='remember')s.rememberLimitOrderTimeInForce(id,tif);if(op==='forget')s.forgetLimitOrderTimeInForce(id);if(op==='get')value=s.getPlacedLimitOrderTimeInForce(id);
 cases.push({kind:'tif',op,id,...(tif?{tif}:{}),...(op==='get'?{present:value!==undefined,...(value!==undefined?{value}:{})}:{}),order:Array.from(s._placedLimitOrderTif.keys())});
}
const c=new out.Subject();
for(const [op,id,time] of [['suppress','a',0],['get','a',120000],['get','a',120001],['get','a',0],['suppress','b',1000],['get','b',999],['unsuppress','b',1000],['get','b',1000],['get','never',0]]) {
 now=time;let value;if(op==='suppress')c.suppressCancelledOrder(id);if(op==='unsuppress')c.unsuppressCancelledOrder(id);if(op==='get')value=c.isOrderCancelled(id);
 cases.push({kind:'cancel',op,id,time,...(op==='get'?{value}:{}),keys:Array.from(c._cancelledOrders.keys())});
}
assert.equal(cases.find(x=>x.kind==='cancel'&&x.time===120000).value,true,'age exactly max remains suppressed');
assert.equal(cases.find(x=>x.kind==='privacy'&&x.options.key).public,true,'signing without identity is public');
const output=JSON.stringify({sourceSha256:crypto.createHash('sha256').update(raw).digest('hex'),scope:'actual source member bodies and field initializers; constructor and transport excluded',cases},null,2)+'\n';
const target=path.join(__dirname,typed?'../tests/fixtures/exchange-state-typed.json':'../tests/fixtures/exchange-state.json');
if(require.main===module) {
 if(process.argv.includes('--capture'))fs.writeFileSync(target,output,{flag:'wx'});else assert.equal(output,fs.readFileSync(target,'utf8'),'exchange state source drift');
 console.log(cases.length+' actual source state cases');
}
module.exports={Subject:out.Subject,setNow:value=>{now=value;},setBeforeNow:hook=>{beforeNow=hook;}};
