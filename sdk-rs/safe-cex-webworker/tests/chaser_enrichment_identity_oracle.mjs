// Field-identity supplement; original chaser enrichment corpus is unchanged.
// Capture validation: Node 26.7.0, darwin/arm64; CI executes on its own host.
import fs from 'node:fs';
import vm from 'node:vm';
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
const require=createRequire(import.meta.url);
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const root=new URL('../../../packages/safe-cex/src/',import.meta.url);
const types=ts.createSourceFile('types.ts',fs.readFileSync(new URL('types.ts',root),'utf8'),ts.ScriptTarget.Latest,true);
const order=types.statements.find(n=>ts.isTypeAliasDeclaration(n)&&n.name.text==='Order');
assert.ok(order&&ts.isTypeLiteralNode(order.type),'ENRICHMENT-IDENTITY explicit Order shape');
function keys(type){return type.members.map(n=>{assert.ok(ts.isPropertySignature(n)&&n.name&&ts.isIdentifier(n.name),'ENRICHMENT-IDENTITY property shape');return n.name.text;}).sort();}
assert.deepEqual(keys(order.type),['id','parentId','status','symbol','type','side','price','amount','filled','remaining','reduceOnly','close','info','clientOrderId','timeInForce','lastUpdate','triggerType','tpTriggerType','slTriggerType','takeProfit','stopLoss','extra'].sort(),'ENRICHMENT-IDENTITY Order field census');
const extra=order.type.members.find(n=>n.name.text==='extra').type;
assert.ok(ts.isTypeLiteralNode(extra),'ENRICHMENT-IDENTITY explicit extra shape');
assert.deepEqual(keys(extra),['remainingNative','filledInPlacementUnits','remainingNotional','conditionalOrderPrice','rejectReason','isFromChaser','chaserId','chaserAttempts','chaserMaxAttempts'].sort(),'ENRICHMENT-IDENTITY extra field census');
function values(name){const e=types.statements.find(n=>ts.isEnumDeclaration(n)&&n.name.text===name);assert.ok(e,'ENRICHMENT-IDENTITY enum '+name);return e.members.map(n=>{assert.ok(n.initializer&&ts.isStringLiteral(n.initializer),'ENRICHMENT-IDENTITY literal enum');return n.initializer.text;});}
const status=values('OrderStatus'),kind=values('OrderType'),side=values('OrderSide'),tif=values('OrderTimeInForce'),trigger=values('OrderStopTriggerType'),reject=values('OrderRejectReason');
const path=new URL('webworker/postProcessors/ChaserOrderEnrichmentPostprocessor.ts',root);
const ast=ts.createSourceFile(path.pathname,fs.readFileSync(path,'utf8'),ts.ScriptTarget.Latest,true);
const source=ast.statements.filter(n=>!ts.isImportDeclaration(n)).map(n=>n.getText(ast)).join('\n').replace(/\bexport /g,'');
const context=vm.createContext({BasePostprocessor:class {}});
vm.runInContext(ts.transpileModule(source+'\nglobalThis.Processor=ChaserOrderEnrichmentPostprocessor;', {compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);
const rows=[];
for(let i=0;i<12;i++){
 const input={activeChasers:[{id:'owner',symbol:'CHASER',size:31,side:'buy',maxAttempts:37,attempts:41,isStopped:false,stopReason:'',currentOrderIds:['owned']}],orders:['owned','unowned'].map((id,j)=>({
  id,parentId:'parent-'+i+'-'+j,clientOrderId:'client-'+i+'-'+j,status:status[i%status.length],symbol:'SYMBOL-'+i+'-'+j,type:kind[i%kind.length],side:side[i%side.length],
  price:101.25,amount:102.5,filled:103.75,remaining:104.125,reduceOnly:i%2===0,close:i%2!==0,info:{identity:'info-'+i+'-'+j},timeInForce:tif[i%tif.length],lastUpdate:105.25,
  triggerType:trigger[i%trigger.length],tpTriggerType:trigger[(i+1)%trigger.length],slTriggerType:trigger[(i+2)%trigger.length],takeProfit:106.5,stopLoss:107.75,
  extra:{remainingNative:108.125,filledInPlacementUnits:109.25,remainingNotional:110.5,conditionalOrderPrice:111.75,rejectReason:reject[i%reject.length],isFromChaser:false,chaserId:'old-owner',chaserAttempts:112.125,chaserMaxAttempts:113.25}
 }))};
 const store=structuredClone(input);
 new context.Processor().process(store);
 rows.push({input,expected:structuredClone(store.orders)});
}
function pairs(type,records){
 const groups=new Map();
 for(const member of type.members){const key=member.type.getText(types);const group=groups.get(key)||[];group.push(member.name.text);groups.set(key,group);}
 let count=0;
 for(const names of groups.values())for(let i=0;i<names.length;i++)for(let j=i+1;j<names.length;j++){
  const a=names[i],b=names[j];count++;
  assert.ok(records.some(r=>Object.hasOwn(r,a)&&Object.hasOwn(r,b)&&!Object.is(r[a],r[b])),`ENRICHMENT-IDENTITY distinguish ${a}/${b}`);
 }
 return count;
}
const records=rows.flatMap(r=>r.expected);
assert.equal(records.length,24,'ENRICHMENT-IDENTITY total output count');
for(const row of rows)assert.equal(row.expected.length,2,'ENRICHMENT-IDENTITY per-case output count');
for(const [field,allowed,seen] of [
 ['status',status,records.map(r=>r.status)],['type',kind,records.map(r=>r.type)],['side',side,records.map(r=>r.side)],
 ['timeInForce',tif,records.map(r=>r.timeInForce)],['triggerType',trigger,records.flatMap(r=>[r.triggerType,r.tpTriggerType,r.slTriggerType])],
 ['rejectReason',reject,records.map(r=>r.extra.rejectReason)]
])assert.deepEqual([...new Set(seen)].sort(),[...allowed].sort(),'ENRICHMENT-IDENTITY enum coverage '+field);
const pairCounts=[pairs(order.type,records),pairs(extra,records.map(r=>r.extra))];
assert.deepEqual(pairCounts,[31,15],'ENRICHMENT-IDENTITY compatible pair census');
process.stdout.write(JSON.stringify({label:'chaser-enrichment-field-identity',pairCounts,rows}));
