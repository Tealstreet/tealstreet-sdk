'use strict';
const assert=require('node:assert/strict'),fs=require('node:fs'),vm=require('node:vm'),path=require('node:path');
const root=path.resolve(__dirname,'../../..');
const requireRoot=require('node:module').createRequire(path.join(root,'package.json'));
const ts=requireRoot(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
function load(file,deps) {
 const source=fs.readFileSync(path.join(root,'packages/safe-cex/src',file),'utf8'),exports={};
 vm.runInNewContext(ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,
 {exports,require:id=>{assert.ok(Object.hasOwn(deps,id),id);return deps[id];}});
 return exports;
}
const utility=load('utils/updateOrderBook.ts',{});
const functions=load('exchanges/binance/binance.orderbook.ts',{'../../utils/updateOrderBook':utility});
const source=fs.readFileSync(path.join(root,'packages/safe-cex/src/exchanges/binance/binance.orderbook.ts'),'utf8');
const ast=ts.createSourceFile('binance.orderbook.ts',source,ts.ScriptTarget.Latest,true),expressions=new Map();
let levelsExpression;
function visit(node) {
 if(ts.isElementAccessExpression(node)&&node.expression.getText(ast)==='bids'&&node.argumentExpression.getText(ast)==='i') {levelsExpression=node.getText(ast);assert.equal(levelsExpression,'bids[i]');}
 if(ts.isElementAccessExpression(node)&&node.expression.getText(ast)==='o') {
  const index=Number(node.argumentExpression.getText(ast));
  assert.ok(index===0||index===1);const text=node.getText(ast);assert.equal(text,`o[${index}]`);
  expressions.set(index,text);
 }
 ts.forEachChild(node,visit);
}
visit(ast);assert.equal(expressions.size,2);assert.ok(levelsExpression);
function bits(value) { assert.equal(typeof value,'number');if(Number.isNaN(value))return'7ff8000000000000';const bytes=Buffer.alloc(8);bytes.writeDoubleBE(value);return bytes.toString('hex'); }
const state=input=>Object.fromEntries([['snapshot','snapshotLastUpdateId'],['applied','lastAppliedUpdateId']].filter(([key])=>Object.hasOwn(input,key)).map(([key,field])=>[field,Number(input[key])]));
const observedState=value=>({snapshot:value.snapshotLastUpdateId===undefined?['unset']:['number',bits(value.snapshotLastUpdateId)],applied:value.lastAppliedUpdateId===undefined?['unset']:['number',bits(value.lastAppliedUpdateId)]});
const seed=()=>({bids:[{price:10,amount:1,total:41},{price:9,amount:1,total:42}],asks:[{price:11,amount:1,total:43},{price:12,amount:1,total:44}]});
const book=value=>Object.fromEntries(['bids','asks'].map(side=>[side,value[side].map(row=>[bits(row.price),bits(row.amount),bits(row.total)])]));
function cell(value) {
 if(value===undefined)return['undefined'];if(value===null)return['null'];
 if(typeof value==='number')return['number',bits(value)];if(typeof value==='boolean')return['bool',value];
 if(typeof value==='string')return['string',Array.from({length:value.length},(_,i)=>value.charCodeAt(i))];
 if(Array.isArray(value))return['array',value.map(cell)];
 return['object',Object.entries(value).map(([key,value])=>[cell(key)[1],cell(value)])];
}
const result={};
const snapshots=[[],[[]],[['1']],[['2','-0'],['1','1e400'],['2','NaN']],[['0x10','12tail','unused']],[['😀1','-Infinity']]];
result.load_binance_order_book_snapshot=[];
for(const rows of snapshots) for(const side of ['bids','asks'])for(const id of ['-0','NaN','Infinity']) {
 const input={bids:[],asks:[],id,state:{snapshot:'1',applied:'99'}};input[side]=rows;
 const data=seed(),sync=state(input.state);
 functions.loadBinanceOrderBookSnapshot(data,{bids:input.bids,asks:input.asks,lastUpdateId:Number(id)},sync);
 result.load_binance_order_book_snapshot.push({input,output:{book:book(data),state:observedState(sync)}});
}
result.get_buffered_binance_depth_updates=[];
const buffers=['[]','[{"u":11},{"u":10},{"u":12},{"u":11}]','[{"u":1e400},{"u":-1e400},{"u":-0},{"u":"Infinity"}]',
 '[{},{"u":null},{"u":false},{"u":true},{"u":[]},{"u":[12]},{"u":{}}]',
 '[{"u":12},{"u":{"toString":null}},{"u":13}]'];
for(const raw of buffers)for(const snapshot of ['10','-0','NaN','Infinity','-Infinity']) {
 const input={raw,snapshot},updates=JSON.parse(raw);let output;
 try {const rows=functions.getBufferedBinanceDepthUpdates(updates,Number(snapshot));output={ok:true,indices:rows.map(row=>updates.indexOf(row)),values:rows.map(row=>cell(row.u))};}
 catch(error){output={ok:false,name:error.name};}
 result.get_buffered_binance_depth_updates.push({input,output});
}
const base={u:12,U:11,pu:10,b:[['9','5'],['10','0'],['8','2']],a:[['12','0'],['11','3']]};
const updates=[JSON.stringify(base)];
for(const [key,value]of[['u',10],['U',13],['u','NaN'],['U',{toString:null}],['b',[[9,'-0']]],['b',[[9,'NaN']]],['a',[[11,'Infinity']]],['b',[[9,2],null]],['b',{length:2,0:[9,2],1:[8,3]}],['b','12'],['b','😀1'],['b',[[{toString:null},1]]],['a',[[12,3],[11,{toString:null}]]]])updates.push(JSON.stringify({...base,[key]:value}));
for(const [key,literal]of[['u','1e400'],['u','-1e400'],['pu','1e400'],['pu','-1e400']])updates.push('{'+Object.entries(base).map(([name,value])=>JSON.stringify(name)+':'+(name===key?literal:JSON.stringify(value))).join(',')+'}');
result.apply_binance_depth_update=[];
for(const raw of updates)for(const profile of [{state:{snapshot:'10',applied:'10'},previous:true},{state:{snapshot:'10'},previous:false}]) {
 const input={raw,...profile},data=seed(),sync=state(input.state);let outcome;
 try{outcome={value:functions.applyBinanceDepthUpdate(data,JSON.parse(raw),sync,{requirePreviousFinalUpdateId:profile.previous})};}catch(error){outcome={error:error.name};}
 result.apply_binance_depth_update.push({input,output:{result:outcome,book:book(data),state:observedState(sync)}});
}
result.depth_index=[];
const cells=['[]','["1","-0"]','[1e400,-0]','[null,true]','[{"n":1e400,"z":-0},["😀","-0"]]','{"0":"12tail","1":false}','"12"'];
for(const raw of cells)for(const index of[0,1]) {
 const input={raw,index},o=JSON.parse(raw),value=vm.runInNewContext(expressions.get(index),{o});
 result.depth_index.push({input,output:cell(value)});
}
for(const raw of ['[["1","2"],["3","4"],["5","6"]]','{"length":3,"0":[1,2],"2":[5,6]}'])for(const index of[0,1,2,3]) {
 const input={raw,index,role:'levels'},bids=JSON.parse(raw),value=vm.runInNewContext(levelsExpression,{bids,i:index});
 result.depth_index.push({input,output:cell(value)});
}
// The private string-index adapter feeds parseFloat immediately. Observe that
// real numeric consumption for split surrogate units, not standalone UTF16 identity.
for(const raw of ['"😀1"','"1😀"'])for(const index of[0,1]) {
 const input={raw,index,consumer:'parseFloat'},o=JSON.parse(raw),value=vm.runInNewContext(expressions.get(index),{o});
 result.depth_index.push({input,output:{consumerBits:bits(parseFloat(value))}});
}
assert.deepEqual(Object.fromEntries(Object.entries(result).map(([name,rows])=>[name,rows.length])),{
 load_binance_order_book_snapshot:36,get_buffered_binance_depth_updates:25,apply_binance_depth_update:36,depth_index:26});
process.stdout.write(JSON.stringify(result));
