import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {createRequire} from 'node:module';
const require=createRequire(import.meta.url),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const root=new URL('../../../',import.meta.url);
const functions={};
for(const [file,name]of [['binance','getBinanceResponseMessage'],['binancepm','getBinancePMResponseMessage']]) {
 const ast=ts.createSourceFile(file,fs.readFileSync(new URL(`packages/safe-cex/src/exchanges/binance/${file}.exchange.ts`,root),'utf8'),ts.ScriptTarget.Latest,true);
 const node=ast.statements.filter(ts.isVariableStatement).flatMap(n=>[...n.declarationList.declarations]).find(n=>n.name.getText(ast)===name);assert.ok(node);
 const exports={};vm.runInNewContext(ts.transpileModule('exports.fn='+node.initializer.getText(ast),{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{exports});functions[file]=exports.fn;
}
const values=[undefined,null,false,true,0,-1,1e21,'',' ',[],[null],['x'],['x','y'],{}, {toString:null},[{toString:null}], 'a&b', 'é🦀'];
const inputs=[undefined,null,false,true,0,1,'x',[],[{}],{}];
for(const code of values)for(const msg of values)for(const message of values)inputs.push({code,msg,message,error:'fallback'});
for(const error of values)inputs.push({error},{code:null,error},{msg:[],message:error},{msg:false,message:null,error});
const cases=[];
for(const [variant,fn]of Object.entries(functions))for(const input of inputs){let expected;try{expected={value:fn(input)};}catch(e){expected={error:e.name};}cases.push({variant,input,expected});}
process.stdout.write(JSON.stringify(cases));
