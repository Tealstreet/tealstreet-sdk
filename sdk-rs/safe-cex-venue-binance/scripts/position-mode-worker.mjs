import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {createRequire} from 'node:module';
const require=createRequire(import.meta.url),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const file=new URL('../../../packages/safe-cex/src/exchanges/binance/binance.exchange.ts',import.meta.url);
const ast=ts.createSourceFile(file.pathname,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
const klass=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text==='BinanceExchange');
assert.ok(klass);
const member=klass.members.find(n=>n.name?.getText(ast)==='afterMethodCalledInWorker');
assert.ok(member);
const exports={};
vm.runInNewContext(ts.transpileModule(`exports.fn=function(...args){return (${member.initializer.getText(ast)})(...args);}`,{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{exports});
const hedges=['null','false','true','0','-0','""','"false"','[]','{}','1e400','-1e400','1e-400','-1e-400','1','-1'];
const symbols=['null','false','0','""','"BTCUSDT"','[]','{}','"💱"','"constructor"'];
const inputs=['[]',...hedges.map(h=>`[${h}]`),...hedges.flatMap(h=>symbols.map(s=>`[${h},${s}]`))];
const cases=[];
for(const method of ['changePositionMode','fetchPositionMode',''])for(const rawArgs of inputs)for(const reject of [false,true]){
 const trace=[],error=RangeError('fixture confirmation');let failure=null;
 const instance={applyConfirmedPositionMode(hedged,symbol){trace.push({hedged,symbolType:typeof symbol,...(symbol===undefined?{}:{symbol})});if(reject)throw error;}};
 try{const result=exports.fn.call(instance,{unused:true},method,...JSON.parse(rawArgs));assert.equal(result,undefined);}catch(e){failure={same:e===error,name:e.name,message:e.message};}
 cases.push({method,rawArgs,reject,trace,error:failure});
}
assert.equal(cases.length,906);
process.stdout.write(JSON.stringify(cases));
