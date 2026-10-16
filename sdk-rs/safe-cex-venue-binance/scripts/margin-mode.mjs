import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {createRequire} from 'node:module';
const require=createRequire(import.meta.url),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const file=new URL('../../../packages/safe-cex/src/exchanges/binance/binancepm.exchange.ts',import.meta.url);
const ast=ts.createSourceFile(file.pathname,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
const klass=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text==='BinancePMExchange');
const member=klass.members.find(n=>n.name?.getText(ast)==='changeMarginType');assert.ok(member);
const exports={};vm.runInNewContext(ts.transpileModule(`exports.fn=function(...args){return (${member.initializer.getText(ast)})(...args);}`,{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{exports});
const cases=[];
for(const isolated of [false,true])for(const symbol of [undefined,'','BTCUSDT','constructor','💱'])for(const emitReject of [false,true]){
 const trace=[],sentinel=Object.assign(new RangeError('fixture emitter'),{code:'fixture-code',response:{status:503,data:{msg:'detail'}}});
 let rejected=false,error;
 try{await exports.fn.call({emitAndLogError(message){trace.push(message);if(emitReject)throw sentinel;}},isolated,symbol);}catch(e){rejected=true;error={name:e.name,message:e.message,same:e===sentinel,code:e.code??null,response:e.response??null};}
 assert.equal(rejected,true,'PM margin mode always rejects');
 cases.push({isolated,symbol:symbol??null,emitReject,trace,error});
}
assert.equal(cases.length,20);
process.stdout.write(JSON.stringify(cases));
