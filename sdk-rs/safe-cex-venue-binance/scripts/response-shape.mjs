import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {createRequire} from 'node:module';
const require=createRequire(import.meta.url),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const root=new URL('../../../',import.meta.url);
const result=[];
for(const [variant,prefix]of [['binance','Binance'],['binancepm','BinancePM']]) {
 const ast=ts.createSourceFile(variant,fs.readFileSync(new URL(`packages/safe-cex/src/exchanges/binance/${variant}.exchange.ts`,root),'utf8'),ts.ScriptTarget.Latest,true);
 const names=[`describe${prefix}ResponseShape`,`get${prefix}ResponseMessage`,`require${prefix}ArrayResponse`];
 const nodes=ast.statements.filter(ts.isVariableStatement).filter(n=>n.declarationList.declarations.some(n=>names.includes(n.name.getText(ast))));assert.equal(nodes.length,3);
 const exports={};vm.runInNewContext(ts.transpileModule(nodes.map(n=>n.getText(ast)).join('\n')+'\n'+names.map(n=>`exports.${n}=${n};`).join('\n'),{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{exports});
 const inputs=[undefined,null,false,true,0,-1,'x',[],[null,1,{}],{}, {code:null},{msg:[],message:'no'},{code:0,msg:'error'},{code:{toString:null}},{msg:{toString:null}}, {code:[],msg:['x','y']}];
 const inputOrder=new WeakMap();
 const keys=['10','2','a','01','0','4294967295','4294967294','-0','__proto__','toString','b','1e2','1'];
 for(let i=0;i<keys.length;i++)for(let len=0;len<=keys.length;len++){const entries=[...keys.slice(i),...keys.slice(0,i)].slice(0,len).map(k=>[k,1]);const input=Object.fromEntries(entries);inputOrder.set(input,entries);inputs.push(input);}
 for(const input of inputs)for(const endpoint of ['/fapi/v1/test','', 'é()🦀']) {
  let expected;try{const value=exports[names[2]](endpoint,input);assert.equal(value,input);expected={value};}catch(e){expected={error:{name:e.name,message:e.message}};}
  result.push({variant,input,entries:inputOrder.get(input),endpoint,shape:exports[names[0]](input),expected});
 }
}
process.stdout.write(JSON.stringify(result));
