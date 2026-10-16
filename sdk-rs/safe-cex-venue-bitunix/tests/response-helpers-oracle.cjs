'use strict';
const {ts,fs,path,vm,root,assert}=require('./oracle-loader.cjs');
const file=path.join(root,'packages/safe-cex/src/exchanges/bitunix/bitunix.exchange.ts');
const ast=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
const owners=ast.statements.filter(n=>ts.isClassDeclaration(n)&&n.name.text==='BitunixExchange');
assert.equal(owners.length,1,'response actual owner census');
const owner={};
for(const name of ['isRecord','getResponseCandidates']) {
 const members=owners[0].members.filter(n=>n.name?.getText(ast)===name);
 assert.equal(members.length,1,`response actual helper census ${name}`);
 assert(ts.isArrowFunction(members[0].initializer));
 const code=ts.transpileModule(`(function(){return (${members[0].initializer.getText(ast)});})`,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText;
 owner[name]=vm.runInThisContext(code,{filename:file}).call(owner);
}
const bits=n=>{const b=Buffer.alloc(8);b.writeDoubleBE(n);return Number.isNaN(n)?'7ff8000000000000':b.toString('hex');};
function observe(v) {
 if(v===undefined)return {type:'undefined'};
 if(v===null)return {type:'null'};
 if(typeof v==='string')return {type:'string',units:Array.from({length:v.length},(_,i)=>v.charCodeAt(i))};
 if(typeof v==='number')return {type:'number',bits:bits(v)};
 if(Array.isArray(v))return {type:'array',items:v.map(observe)};
 if(typeof v==='object')return {type:'object',entries:Object.keys(v).sort().map(k=>[k,observe(v[k])])};
 return {type:typeof v,value:v};
}
const inputs=[null,'null','false','true','0','-0','1e400','-1e400','""','"abc"','[]','[1,2]','{}','{"data":null}',
 '{"rows":7,"list":6,"positions":5,"positionList":4,"payload":3,"result":2,"data":1}',
 '{"data":{"x":1},"result":{"x":1}}','{"data":4,"result":4}',
 '{"data":[{"data":7},{"rows":9}],"result":false}',
 '{"data":{"result":{"payload":{"list":{"rows":{"data":99}}}}}}',
 '{"unknown":{"data":99},"__proto__":{"rows":17}}',
 '{"data":-0,"result":1e400,"payload":-1e400}',
 '"\\ud800"'];
for(const edge of ['data','result','payload','positionList','positions','list','rows']) {
 for(let depth=1;depth<=6;depth++) {
  let value={marker:`${edge}:${depth}`};for(let i=0;i<depth;i++)value={[edge]:value};inputs.push(JSON.stringify(value));
 }
}
assert.equal(inputs.length,64,'RESPONSE source corpus census');
const rows=inputs.map(raw=>{
 const value=raw===null?undefined:JSON.parse(raw);
 const candidates=owner.getResponseCandidates(value);
 // Resolve each output to an input path by identity; do not infer native order from values.
 function location(target,current=value,at='$') {
  if(target===current || (typeof target==='number'&&Object.is(target,current)))return at;
  if(current && typeof current==='object')for(const key of Object.keys(current)) {
   const found=location(target,current[key],`${at}/${key}`);if(found!==null)return found;
  }
  return null;
 }
 return {raw,record:owner.isRecord(value),outputs:candidates.map(observe),paths:candidates.map(v=>v && typeof v==='object'?location(v):null)};
});
process.stdout.write(JSON.stringify(rows));
