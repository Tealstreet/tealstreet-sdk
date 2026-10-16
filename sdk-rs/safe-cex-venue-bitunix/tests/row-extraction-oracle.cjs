'use strict';
const {ts,fs,path,vm,root,assert}=require('./oracle-loader.cjs');
const file=path.join(root,'packages/safe-cex/src/exchanges/bitunix/bitunix.exchange.ts');
const ast=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
const owners=ast.statements.filter(n=>ts.isClassDeclaration(n)&&n.name.text==='BitunixExchange');
assert.equal(owners.length,1,'row owner census');
const owner={};
for(const name of ['isRecord','getResponseCandidates','getFirstValue','extractAccountRow','extractPositionRows']) {
 const members=owners[0].members.filter(n=>n.name?.getText(ast)===name);assert.equal(members.length,1,`row helper ${name}`);assert(ts.isArrowFunction(members[0].initializer));
 const code=ts.transpileModule(`(function(){return (${members[0].initializer.getText(ast)});})`,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText;
 owner[name]=vm.runInThisContext(code,{filename:file}).call(owner);
}
const bits=n=>{const b=Buffer.alloc(8);b.writeDoubleBE(n);return Number.isNaN(n)?'7ff8000000000000':b.toString('hex');};
function obs(v) {
 if(v===undefined)return {type:'undefined'};
 if(v===null)return {type:'null'};
 if(typeof v==='number')return {type:'number',bits:bits(v)};
 if(typeof v==='function' || v===Object.prototype)return {type:typeof v,string:String(v)};
 if(typeof v==='string')return {type:'string',units:Array.from({length:v.length},(_,i)=>v.charCodeAt(i))};
 if(Array.isArray(v))return {type:'array',items:v.map(obs)};
 if(typeof v==='object')return {type:'object',entries:Object.keys(v).sort().map(k=>[k,obs(v[k])])};
 return {type:typeof v,value:v};
}
const capture=fn=>{try{return {ok:obs(fn())};}catch(e){return {error:e.name};}};
const first=[];
const fields=[['a','b'],['b','a'],[],['missing','constructor'],['toString'],['__proto__'],['a_b'],['aB']];
const objects=['{}','{"a":null,"b":2}','{"a":"","b":3}','{"a":false,"b":4}','{"a":0,"b":5}','{"a":-0,"b":6}','{"a":[],"b":7}','{"a":{},"b":8}','{"a":1e400,"b":9}','{"a_b":10,"aB":11}','{"constructor":null,"toString":"","__proto__":false}','{"a":"\\ud800"}'];
for(const raw of objects)for(const keys of fields)first.push({raw,keys,result:obs(owner.getFirstValue(JSON.parse(raw),keys))});
const inputs=[null,'null','{}','[]','{"symbol":"solo"}',
 '[{"symbol":"A","marginCoin":"USDT","available":"11"},{"symbol":"B","marginCoin":"BTC","available":"22"}]',
 '{"data":[null,[],{}, {"available":null}, {"symbol":false,"instId":"skip"}, {"symbol":0,"instId":"skip"}, {"symbol":"","instId":"C"}]}',
 '{"result":[{"symbol":"first","available":"1"}],"data":[{"symbol":"earlier","available":"2"}]}',
 '{"data":[{},null],"result":[{"contract":"X"},{"instrument":"Y"}]}',
 '{"availableBalance":0,"data":[{"marginCoin":"USDT","available":"child"}]}',
 '{"data":[{"marginCoin":{"toString":1},"symbol":"X"}]}',
 '{"data":[{"marginCoin":0,"symbol":[],"qty":-0},{"marginCoin":false,"symbol":{},"qty":1e400}]}',
 '{"data":{"data":{"data":{"data":{"data":[{"symbol":"too-deep","available":1}]}}}}}',
 '{"data":[{"margin":1},{"frozen":2},{"positionMode":3},{"crossUnrealizedPNL":4},{"isolationUnrealizedPNL":5}]}',
 '{"data":[{"marginCoin":"usdt","symbol":"dup"},{"marginCoin":"USDT","symbol":"dup"}]}',
 '{"data":[{"marginCoin":"ß","symbol":"unicode"},{"marginCoin":"SS","symbol":"second"}]}',
 '{"data":[{"marginCoin":"USDT","symbol":"\\ud800"}]}'];
const rows=[];
for(const raw of inputs)for(const coin of ['USDT','btc','','SS']) {
 const value=raw===null?undefined:JSON.parse(raw);
 rows.push({raw,coin,account:capture(()=>owner.extractAccountRow(value,coin)),positions:obs(owner.extractPositionRows(value))});
}
assert.equal(first.length,96,'FIRST fixed source total');assert.equal(rows.length,68,'ROWS fixed source total');
process.stdout.write(JSON.stringify({first,rows}));
