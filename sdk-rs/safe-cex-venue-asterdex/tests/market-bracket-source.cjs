'use strict';
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const {ts,root,values}=require('../scripts/source.cjs');
const parse=file=>ts.createSourceFile(file,fs.readFileSync(path.join(root,file),'utf8'),ts.ScriptTarget.Latest,true);
const tree=parse('packages/safe-cex/src/exchanges/asterdex/asterdex.exchange.ts');
const cls=tree.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text==='AsterDEXExchange');
const names=['getFapiPath','fetchMarkets','fetchLeverageBrackets','transformMarket','extractFilterValue'];
const members=names.map(name=>{const n=cls.members.find(n=>n.name?.getText(tree)===name);assert(n,name);return n.getText(tree)}).join('\n');
const compile=text=>ts.transpileModule(text,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText;
const precision={};new Function('exports',compile(fs.readFileSync(path.join(root,'packages/safe-cex/src/utils/ccxt_helpers/precision.ts'),'utf8')))(precision);
const logTree=parse('packages/logging/src/LogSeverity.ts');
const enums=logTree.statements.filter(ts.isEnumDeclaration).map(n=>n.getText(logTree)).join('\n');
const result={};new Function('exports','precisionToString','getAsterDEXFapiPath',compile(enums+'\nexport class Source {'+members+'}'))(result,precision.precisionToString,values.getAsterDEXFapiPath);
(async()=>{
 const {market,cases}=JSON.parse(fs.readFileSync(0,'utf8'));assert.equal(cases.length,15);
 const rows=[];
 for(const version of ['v1','v3'])for(const c of cases){
  const instance=new result.Source(),requests=[],logs=[];instance.apiVersion=version;
  instance.publicAPI={get:async url=>{requests.push(url);return {data:{symbols:[market]}}}};
  instance.primaryAPI={get:async url=>{requests.push(url);return {data:c.data}}};
  instance.log=(message,severity)=>logs.push({message,severity});
  const markets=await instance.fetchMarkets();
  rows.push({version,name:c.name,max:markets[0].limits.leverage.max,requests,logs});
 }
 process.stdout.write(JSON.stringify(rows));
})().catch(error=>{process.stderr.write(error.stack);process.exitCode=1});
