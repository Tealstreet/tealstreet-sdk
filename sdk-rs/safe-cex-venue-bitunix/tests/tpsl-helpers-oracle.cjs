'use strict';
const {ts,fs,path,vm,root,assert,load}=require('./oracle-loader.cjs');
const file=path.join(root,'packages/safe-cex/src/exchanges/bitunix/bitunix.exchange.ts');
const ast=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
const owners=ast.statements.filter(n=>ts.isClassDeclaration(n)&&n.name.text==='BitunixExchange');assert.equal(owners.length,1);
const owner={};const {OrderStopTriggerType}=load(path.join(root,'packages/safe-cex/src/types.ts'));
for(const name of ['getTpslLegOrderId','getBitunixTriggerType','getBitunixReduceOnly']) {
 const members=owners[0].members.filter(n=>n.name?.getText(ast)===name);assert.equal(members.length,1,`TPSL helper ${name}`);assert(ts.isArrowFunction(members[0].initializer));
 const code=ts.transpileModule(`(function(OrderStopTriggerType){return (${members[0].initializer.getText(ast)});})`,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText;
 owner[name]=vm.runInThisContext(code,{filename:file}).call(owner,OrderStopTriggerType);
}
const inputs=[null,'null','false','true','0','-0','1e400','9007199254740993','""','"abc"','"true"','"FALSE"','" false "','[]','["mark"]','{}','{"toString":1}','"\\ud800"'];
const value=raw=>raw===null?undefined:JSON.parse(raw);
const obs=fn=>{try{const v=fn();return typeof v==='string'?{type:'string',units:Array.from({length:v.length},(_,i)=>v.charCodeAt(i))}:{type:typeof v,value:v};}catch(e){return {error:e.name};}};
const ids=[];
for(const raw of inputs)for(const position of inputs)for(const leg of ['tp','sl'])for(const disambiguate of [false,true])ids.push({raw,position,leg,disambiguate,result:obs(()=>owner.getTpslLegOrderId(value(raw),value(position),leg,disambiguate))});
const triggers=[];
const triggerInputs=[...inputs,...['MARK','mark_price','LAST','last_price',' LAST ','laſt'].map(JSON.stringify)];
for(const raw of triggerInputs)for(const choice of ['none','mark','last'])triggers.push({raw,choice,result:obs(()=>owner.getBitunixTriggerType(choice==='none'?undefined:choice==='mark'?OrderStopTriggerType.MarkPrice:OrderStopTriggerType.LastPrice,value(raw)))});
const cache=[['abc',false],['true',true],['0',false],['Infinity',true],['',true],['[object Object]',false],['mark',true]];
owner.orderReduceOnlyCache=new Map(cache);
const reduce=[];
for(const raw of inputs)for(const client of inputs)for(const fallback of [false,true])reduce.push({raw,client,fallback,result:obs(()=>owner.getBitunixReduceOnly(value(raw),value(client),fallback))});
assert.equal(ids.length,1296);assert.equal(triggers.length,72);assert.equal(reduce.length,648);
process.stdout.write(JSON.stringify({ids,triggers,reduce,cache}));
