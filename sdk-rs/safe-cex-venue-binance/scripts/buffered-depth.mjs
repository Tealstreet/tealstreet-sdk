import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {createRequire} from 'node:module';
const require=createRequire(import.meta.url);
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const root=new URL('../../../',import.meta.url);
const text=fs.readFileSync(new URL('packages/safe-cex/src/exchanges/binance/binance.orderbook.ts',root),'utf8');
const ast=ts.createSourceFile('orderbook.ts',text,ts.ScriptTarget.Latest,true);
const node=ast.statements.filter(ts.isVariableStatement).find(s=>s.declarationList.declarations.some(d=>d.name.getText(ast)==='getBufferedBinanceDepthUpdates'));
assert.ok(node);
const alias=ast.statements.find(n=>ts.isTypeAliasDeclaration(n)&&n.name.text==='BinanceDepthUpdate');
assert.equal(alias.type.getText(ast),'Record<string, any>');
const exports={};vm.runInNewContext(ts.transpileModule(node.getText(ast),{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{exports});
const values=[undefined,null,false,true,-1,-0,0,1,12,[],[12],['12','3'],{}, {toString:null},[{toString:null}],{valueOf:0},'',' ','0x10','0b11','12abc','-0','Infinity','-Infinity','1e999','1e-999','NaN'];
for(let i=0;i<256;i++)values.push(String.fromCharCode(i)+'12');
const inputs=values.map(u=>JSON.stringify([{u,marker:0},{u:12,marker:1},{u,marker:2},{u:0,marker:3}]));
inputs.push('[]','[{"u":1e400},{"u":-1e400},{"u":-0}]');
const cases=[];
for(const input of inputs)for(const snapshot of [NaN,-Infinity,-1,-0,0,1,12,15.5,Infinity]) {
 const updates=JSON.parse(input);let expected;
 try{expected={indices:exports.getBufferedBinanceDepthUpdates(updates,snapshot).map(r=>updates.indexOf(r))};}catch(e){expected={error:e.name};}
 cases.push({input,snapshot:Object.is(snapshot,-0)?'-0':String(snapshot),expected});
}
process.stdout.write(JSON.stringify(cases));
