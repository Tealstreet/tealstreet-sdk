const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path');
const {ts,root,load}=require('../tests/configuration.cjs');
const {ENDPOINTS}=load(path.join(root,'packages/safe-cex/src/exchanges/bitrue/bitrue.types.ts'));
const file=path.join(root,'packages/safe-cex/src/exchanges/bitrue/bitrue.exchange.ts');
const source=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
const klass=source.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text==='BitrueExchange');
const compiled=new Map();
function method(name,context){if(!compiled.has(name)){const members=klass.members.filter(n=>n.name?.getText(source)===name);assert.equal(members.length,1,'actual initializer '+name);compiled.set(name,new Function('ENDPOINTS',ts.transpileModule(`return (${members[0].initializer.getText(source)});`,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText));}return compiled.get(name).call(context,ENDPOINTS);}

const inputs=[null,'null','{}','{"code":0,"sentinel":17}','{"code":"0","sentinel":19}','{"code":200,"sentinel":23}','{"code":"200","sentinel":29}','{"code":"00","msg":"bad-code"}','{"code":false,"msg":"rejected"}','{"code":1,"msg":0}','{"code":1,"msg":37}','{"code":1,"msg":[3,7]}','{"code":1,"msg":{}}','{"code":1,"msg":{"toString":1}}'];
const faults=[null,{message:'request rejected'},{message:'fallback',response:{data:{msg:'response rejected'}}},{message:'fallback',response:{data:{msg:0}}},{message:'fallback',response:{data:{msg:{tag:41}}}}];
(async()=>{const rows=[];
for(const raw of inputs)for(const fault of faults)for(const emitFails of [false,true]){
 const calls=[],events=[],emitted=[];
 const context={xhr:{get:async(url)=>{calls.push(url);events.push('request');await Promise.resolve();events.push('response');if(fault)throw Object.assign(new Error(fault.message),fault);return {data:raw===null?undefined:JSON.parse(raw)};}},emitAndLogError:value=>{emitted.push(value);events.push('emit');if(emitFails)throw Object.assign(new Error('emitter rejected'),{name:'SubscriberError'});}};
 for(const name of ['isSuccessCode','fetchBalanceAndPositionsRaw'])context[name]=method(name,context);
 let result;try{const value=await context.fetchBalanceAndPositionsRaw();result=value===undefined?{kind:'undefined'}:{kind:'data',value};}catch(e){result={kind:'error',name:e.name,message:e.message};}
 rows.push({input:{raw,fault,emitFails},calls,events,emitted,result});
}
assert.equal(rows.length,140,'BITRUE-ACCOUNT-READ source total');
const balances=[];
const balanceInputs=[null,'null','false','{"data":{"account":[]}}','{"data":{"account":[{"marginCoin":"USDT","totalEquity":31,"accountNormal":7,"sumOpenRealizedAmount":-23}]}}','{}'];
for(const raw of balanceInputs)for(const reject of [false,true]){
 const events=[],stored={total:101,free:37,used:64,upnl:-11},context={store:{balance:stored}};
 context.fetchBalanceAndPositionsRaw=async()=>{events.push('fetch');await Promise.resolve();context.store.balance={total:211,free:43,used:168,upnl:-19};events.push('resolve');if(reject)throw new Error('fetch rejected');return raw===null?undefined:JSON.parse(raw);};
 context.parseBalance=method('parseBalance',context);context.fetchBalance=method('fetchBalance',context);
 let result;try{const value=await context.fetchBalance();result={kind:'balance',stored:value===context.store.balance,fields:[value.total,value.free,value.used,value.upnl].map(v=>{const b=Buffer.alloc(8);b.writeDoubleBE(v);return b.toString('hex');}),currencies:value.currencies===undefined?null:Object.keys(value.currencies),info:value.info===undefined?null:value.info};}catch(e){result={kind:'error',name:e.name,message:e.message};}
 balances.push({input:{raw,reject},events,result});
}
const supplements=[];
for(const raw of ['{"code":0,"extra":"\\ud800"}','{"code":0,"extra":'+ '['.repeat(160)+'3'+']'.repeat(160)+'}']){
 const context={xhr:{get:async()=>({data:JSON.parse(raw)})},emitAndLogError:()=>{throw new Error('unexpected emission');}};
 for(const name of ['isSuccessCode','fetchBalanceAndPositionsRaw'])context[name]=method(name,context);
 const result=await context.fetchBalanceAndPositionsRaw();assert.equal(result.code,0,'source admits CHOSEN ingress supplement');supplements.push(raw);
}
process.stdout.write(JSON.stringify({header:require('./capture-header.cjs')(),rows,balances,supplements}));
})().catch(e=>{console.error(e);process.exitCode=1;});
