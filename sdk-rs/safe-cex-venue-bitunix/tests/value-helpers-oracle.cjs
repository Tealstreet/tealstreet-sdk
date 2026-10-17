const {ts,fs,path,vm,root,assert} = require('./oracle-loader.cjs');
const file = path.join(root,'packages/safe-cex/src/exchanges/bitunix/bitunix.exchange.ts');
const ast = ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
const owner = ast.statements.filter(n=>ts.isClassDeclaration(n)&&n.name.text==='BitunixExchange');
assert.equal(owner.length,1,'actual class census');
const helpers = {};
for(const name of ['parseFiniteNumber','isApiTradingSupported','hasFiniteTriggerPrice']) {
 const nodes=owner[0].members.filter(n=>n.name?.getText(ast)===name);
 assert.equal(nodes.length,1,`actual helper census ${name}`);
 assert(ts.isArrowFunction(nodes[0].initializer));
 const code=ts.transpileModule(`(${nodes[0].initializer.getText(ast)})`,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText;
 helpers[name]=vm.runInThisContext(code,{filename:file});
}
const bits = n => { const b=Buffer.alloc(8); b.writeDoubleBE(Number.isNaN(n)?NaN:n); return Number.isNaN(n)?'7ff8000000000000':b.toString('hex'); };
const capture = fn => {try {const value=fn();return {type:typeof value,value:typeof value==='number'?bits(value):value};}catch(e){return {error:e.name,message:e.message};}};
const raws=[null,'null','true','false','0','-0','1','-2','1e400','-1e400','5e-324','9007199254740993',
 ...['',' ','\ufeff','\u0085','false','FALSE','  FaLsE  ','0','NO','n','disabled','unsupported','true','yes','-0','0x10','1x','Infinity','NaN','é','😀'].map(JSON.stringify),
 '[]','[null]','[false]','["2"]','[["-0"]]','[1,2]','{}','{"valueOf":2}','{"toString":1}',
 '{"toString":null}','{"__proto__":null}','"\\ud800"'];
const rows=[];
for(const raw of raws) for(const fallback of [0,-0,17.25,NaN,Infinity]) {
 const value=raw===null?undefined:JSON.parse(raw);
 rows.push({raw,fallback:bits(fallback),parse:capture(()=>helpers.parseFiniteNumber(value,fallback)),
 supported:capture(()=>helpers.isApiTradingSupported(value)),trigger:capture(()=>helpers.hasFiniteTriggerPrice(value))});
}
process.stdout.write(JSON.stringify(rows));
