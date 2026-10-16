import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {createRequire} from 'node:module';
const require=createRequire(import.meta.url),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const root=new URL('../../../packages/safe-cex/src/exchanges/binance/',import.meta.url);
const variants=[['binance','BinanceMarketWebsocket'],['binance','BinancePublicWebsocket'],['binancepm','BinancePMMarketWebsocket'],['binancepm','BinancePMPublicWebsocket'],['binancespot','BinancePublicWebsocket']];
const values=[null,false,true,'1','0','',[],[1],{},0,-1,0.1,1,2,{raw:'1e400'},{raw:'-1e400'},{raw:'-0'},{native:'NaN'},{native:'Infinity'},{native:'-Infinity'}];
const keys=['topic','2','10','01','0','4294967294','4294967295','constructor','__proto__','雪😀','quote"\n\\'];
const cases=[];
for(const [file,klassName] of variants){
 const ast=ts.createSourceFile(file,fs.readFileSync(new URL(file+'.ws-public.ts',root),'utf8'),ts.ScriptTarget.Latest,true);
 const klass=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text===klassName),member=klass.members.find(n=>n.name?.getText(ast)==='subscribe');assert.ok(member);
 const exports={};vm.runInNewContext(ts.transpileModule(`exports.fn=function(){return (${member.initializer.getText(ast)})();}`,{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{exports});
 const fixtures=[];
 for(const key of keys)for(const value of values)for(const action of ['none','remove','activate','append','throw'])fixtures.push({entries:[['z',1],[key,value],['later',0],['last',2]],action});
 fixtures.push({entries:[],action:'none'});
 for(const c of fixtures){
  const topics=Object.fromEntries(c.entries.map(([k,v])=>[k,v&&typeof v==='object'&&'raw'in v?JSON.parse(v.raw):v&&typeof v==='object'&&'native'in v?Number(v.native):v]));
  const sent=[];let error;
  try{exports.fn.call({topics,send(frame){sent.push(frame);if(sent.length===1){if(c.action==='remove')delete topics.last;if(c.action==='activate')topics.later=1;if(c.action==='append')topics.added=1;if(c.action==='throw')throw Error('fixture send error');}}});}catch(e){error=e.message;}
  cases.push({variant:file+':'+klassName,...c,sent,error:error??null});
 }
}
process.stdout.write(JSON.stringify(cases));
