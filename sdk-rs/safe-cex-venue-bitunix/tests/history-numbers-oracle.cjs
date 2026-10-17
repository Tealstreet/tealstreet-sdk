'use strict';
const {ts,fs,path,vm,root,assert} = require('./oracle-loader.cjs');
const helpers={};
for (const [fileName,names] of [['bitunix.order-history.ts',['toOptionalNumber','toNumber']],['bitunix.income.ts',['toNumber']]]) {
 const file=path.join(root,'packages/safe-cex/src/exchanges/bitunix',fileName);
 const ast=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
 for(const name of names) {
  const nodes=ast.statements.filter(ts.isVariableStatement).flatMap(n=>[...n.declarationList.declarations]).filter(n=>n.name.getText(ast)===name);
  assert.equal(nodes.length,1,`actual history helper census ${fileName}:${name}`);
  assert(ts.isArrowFunction(nodes[0].initializer));
  const code=ts.transpileModule(`(${nodes[0].initializer.getText(ast)})`,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText;
  helpers[`${fileName}:${name}`]=vm.runInThisContext(code,{filename:file});
 }
}
const bits=n=>{const b=Buffer.alloc(8);b.writeDoubleBE(n);return Number.isNaN(n)?'7ff8000000000000':b.toString('hex');};
const observe=fn=>{try{const n=fn();return {type:typeof n,...(typeof n==='number'?{bits:bits(n)}:{})};}catch(e){return {error:e.name,message:e.message};}};
const raws=[null,'null','true','false','0','-0','1.25','-2.75','1e400','-1e400','5e-324','9007199254740993',
 ...['',' ','MARKET','NaN','Infinity','-Infinity','-0','-0.0tail','12.75tail','0x10','1e','1e+','1e-2x','.5foo','+.5','-.5','  2.5 ','\ufeff3.25','\u00854.5','\u20285.5','1_000','0b11','1e309','-1e-999','é','😀'].map(JSON.stringify),
 '[]','[null]','[false]','["2.75"]','[["-0tail"]]','[12.5,2]','{}','{"valueOf":2}','{"toString":1}','{"toString":null}','{"__proto__":null}',
 '"\\ud800"','"12.5\\ud800"'];
assert.equal(raws.length,51,'history source input census');
const rows=raws.map(raw=>{const value=raw===null?undefined:JSON.parse(raw);return {raw,results:Object.fromEntries(Object.entries(helpers).map(([name,fn])=>[name,observe(()=>fn(value))]))};});
process.stdout.write(JSON.stringify(rows));
