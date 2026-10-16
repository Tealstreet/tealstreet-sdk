const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const {root}=require('./tables.cjs');
function owner(venue,clear,superDispose){
 const file=path.join(root,`packages/safe-cex/src/exchanges/kucoin/${venue}.ws-private.ts`);
 const ast=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
 const cls=ast.statements.find(ts.isClassDeclaration);
 const member=cls.members.find(n=>n.name?.getText(ast)==='dispose');
 assert.ok(member,'maintained dispose member');
 const code=ts.transpileModule('export class Selected extends Base {'+member.getText(ast)+'}',{
  compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS},
 }).outputText;
 const module={exports:{}};
 class Base{dispose(){superDispose();}}
 new Function('module','exports','Base','clearTimeout',code)(module,module.exports,Base,clear);
 return new module.exports.Selected();
}
const cases=JSON.parse(fs.readFileSync(0,'utf8'));
assert.equal(cases.length,76,'declared disposal floor');
const rows=[];
for(const c of cases){
 const events=[],failure=new TypeError('dispose callback failure');let index=0;
 const object={id:9};
 const value=kind=>kind==='none'?undefined:kind==='zero'?0:kind==='number'?7:object;
 const tag=v=>v===undefined?'none':v===0?'zero':typeof v==='number'?'number':'object';
 let receiver;
 const clear=h=>{events.push('clear:'+tag(h));if(index++===c.failAt)throw failure;};
 receiver=owner(c.venue,clear,()=>{events.push('super');if(c.failSuper)throw failure;});
 receiver.snapshotRefreshTimer=value(c.orders);receiver.orderRefreshTimeout=value(c.orders);receiver.balanceRefreshTimeout=value(c.balance);
 let error=null;
 try{receiver.dispose();}catch(e){error={name:e.name,message:e.message,same:e===failure};}
 rows.push({events,orders:tag(c.venue==='kucoin'?receiver.snapshotRefreshTimer:receiver.orderRefreshTimeout),balance:tag(receiver.balanceRefreshTimeout),error});
}
for(const venue of ['kucoin','kucoinspot']){
 let receiver;const failure=new TypeError('replacement failure');
 receiver=owner(venue,()=>{
  if(venue==='kucoin')receiver.snapshotRefreshTimer=19;
  else receiver.orderRefreshTimeout=19;
  throw failure;
 },()=>assert.fail('super after cancellation failure'));
 receiver.snapshotRefreshTimer=7;receiver.orderRefreshTimeout=7;receiver.balanceRefreshTimeout=9;
 assert.throws(()=>receiver.dispose(),e=>e===failure,'original replacement error');
 assert.equal(venue==='kucoin'?receiver.snapshotRefreshTimer:receiver.orderRefreshTimeout,19,'replacement retained on throw');
 assert.equal(receiver.balanceRefreshTimeout,9,'later slot retained on throw');
}
let receiver=owner('kucoin',()=>{receiver.snapshotRefreshTimer=9;},()=>{});
receiver.snapshotRefreshTimer=7;receiver.dispose();assert.equal(receiver.snapshotRefreshTimer,undefined);
const seen=[];
receiver=owner('kucoinspot',h=>{seen.push(h);if(h===7)receiver.balanceRefreshTimeout=11;},()=>{});
receiver.orderRefreshTimeout=7;receiver.balanceRefreshTimeout=9;receiver.dispose();assert.deepEqual(seen,[7,11]);
receiver=owner('kucoin',()=>assert.fail('falsey NaN cancellation'),()=>{});
receiver.snapshotRefreshTimer=NaN;receiver.dispose();assert.ok(Number.isNaN(receiver.snapshotRefreshTimer));
process.stdout.write(JSON.stringify(rows));
