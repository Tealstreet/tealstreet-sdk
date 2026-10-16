// Actual cache fields and methods, with Date.now captured at its source boundary.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'../../../node_modules/typescript');
const file=path.resolve(__dirname,'../../../packages/safe-cex/src/exchanges/bitget/bitget.ws-private.ts');
const ast=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
const klass=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text==='BitgetPrivateWebsocket');
const names=['terminalOrderTombstones','recentWsOpenOrders','orderTombstoneTtlMs','wsOpenOrderGraceMs','pruneOrderLifecycleCaches','markOrderAsTerminal','markOrderAsOpenFromWs'];
const members=klass.members.filter(n=>names.includes(n.name?.getText(ast)));
assert.deepEqual(members.map(n=>n.name.getText(ast)).sort(),[...names].sort(),'private order lifecycle inventory');
const code=ts.transpileModule('class Source {'+members.map(n=>n.getText(ast)).join('\n')+'}',{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText;
let now=0,clockCalls=0;
const Source=new Function('Date',code+';return Source;')({now:()=>{clockCalls++;return now;}});
const num=n=>Object.is(n,-0)?'-0':String(n);
const entries=m=>[...m].map(([k,v])=>[k,num(v)]);
const cases=[];
function capture(name,initial,operations){
 const receiver=new Source();
 receiver.terminalOrderTombstones=new Map(initial.terminal.map(([k,v])=>[k,Number(v)]));
 receiver.recentWsOpenOrders=new Map(initial.open.map(([k,v])=>[k,Number(v)]));
 clockCalls=0;const states=[];
 for(const op of operations){now=Number(op.now);if(op.kind==='prune')receiver.pruneOrderLifecycleCaches(now);else if(op.kind==='terminal')receiver.markOrderAsTerminal(op.id);else receiver.markOrderAsOpenFromWs(op.id);states.push({terminal:entries(receiver.terminalOrderTombstones),open:entries(receiver.recentWsOpenOrders),clockCalls});}
 cases.push({name,initial,operations,states});
}
const numbers=['-Infinity','-1','-0','0','1','5000','10000','1e20','-1e20','Infinity','NaN'];
for(const now of numbers){
 const seed=numbers.map((n,i)=>['k'+i,n]);
 capture('prune:'+now,{terminal:seed,open:[...seed].reverse()},[{kind:'prune',now}]);
 for(const kind of ['terminal','open'])capture(kind+':'+now,{terminal:[['target','NaN'],['z','0']],open:[['target','0'],['a','Infinity']]},[{kind,id:'target',now},{kind:'prune',now}]);
}
for(const kind of ['terminal','open'])for(const count of [1999,2000,2001])for(const existing of [false,true]){
 const seed=Array.from({length:count},(_,i)=>['k'+i,i%2?'20000':'9000']);
 capture(`boundary:${kind}:${count}:${existing}`,{terminal:kind==='terminal'?seed:[['other','9000'],['keep','20000']],open:kind==='open'?seed:[['other','9000'],['keep','20000']]},[{kind,id:existing?'k0':'new',now:'10000'}]);
}
for(const kind of ['terminal','open']) {
 const fresh=Array.from({length:2001},(_,i)=>['fresh'+i,'20000']);
 const expired=Array.from({length:2001},(_,i)=>['expired'+i,'0']);
 capture(`fresh-over-limit:${kind}`,{terminal:kind==='terminal'?fresh:[],open:kind==='open'?fresh:[]},[{kind,id:'new',now:'10000'}]);
 capture(`opposite-over-limit:${kind}`,{terminal:kind==='open'?expired:[],open:kind==='terminal'?expired:[]},[{kind,id:'new',now:'10000'}]);
}
for(const id of ['constructor','__proto__','toString','1','01','\ufeff','\u0085','🦀',''])capture('keys:'+id,{terminal:[],open:[]},[{kind:'open',id,now:'0'},{kind:'terminal',id,now:'1'},{kind:'open',id,now:'2'}]);
capture('insertion-order',{terminal:[['2','1'],['a','2'],['1','3'],['a','4']],open:[]},[{kind:'terminal',id:'a',now:'0'},{kind:'prune',now:'4'},{kind:'terminal',id:'2',now:'1'}]);
capture('interior-open-removal',{terminal:[],open:[['a','20000'],['target','20000'],['z','20000'],['tail','20000']]},[{kind:'terminal',id:'target',now:'1'}]);
const utf16=[];for(const id of ['\ud800','x\udc00']){const s=new Source();now=1;s.markOrderAsTerminal(id);utf16.push({input:JSON.stringify(id),terminal:JSON.stringify(entries(s.terminalOrderTombstones))});}
process.stdout.write(JSON.stringify({cases,utf16})+'\n');
