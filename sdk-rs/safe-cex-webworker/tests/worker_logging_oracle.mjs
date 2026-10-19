// Live source capture: validated Node 26.7.0 on darwin/arm64; CI executes on its own host.
import fs from 'node:fs';
import vm from 'node:vm';
import {createRequire} from 'node:module';
const require=createRequire(import.meta.url);
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const path=new URL('../../../packages/safe-cex/src/webworker/ExchangeWorker.ts',import.meta.url);
const source=fs.readFileSync(path,'utf8');
const ast=ts.createSourceFile(String(path),source,ts.ScriptTarget.Latest,true);
const names=['isRecord','getWorkerSentryControl','splitWorkerLogExtra'];
const selected=ast.statements.filter(s=>ts.isVariableStatement(s)&&s.declarationList.declarations.some(d=>names.includes(d.name.getText(ast))));
if(selected.length!==3)throw new Error('worker helper source inventory changed');
const context=vm.createContext({});
vm.runInContext(ts.transpileModule(selected.map(s=>s.getText(ast)).join('\n')+'\nglobalThis.helpers={isRecord,getWorkerSentryControl,splitWorkerLogExtra};',{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);
const h=context.helpers;
const values=[null,false,true,0,-0,1,-1,0.1,'',' ','key',[],[1],{}, {x:1}, {toString:null}, {__proto__:null}];
const records=[];
function record(input){records.push({input,isRecord:h.isRecord(input),control:h.getWorkerSentryControl(input),split:h.splitWorkerLogExtra(input)});}
values.forEach(record);
for(const a of values)for(const b of values){
 record({safeCexSentryReportKey:a,safeCexSentryReportCooldownMs:b,safeCexSentryTags:a,safeCexSentryExtra:b});
 record({sentryTags:a,sentryExtra:b,suppressConsoleLog:a,keep:b,constructor:'ordinary',__proto__:null});
}
for(const n of [NaN,Infinity,-Infinity]) records.push({input:{safeCexSentryReportCooldownMs:null},control:h.getWorkerSentryControl({safeCexSentryReportCooldownMs:n}),isRecord:true,split:h.splitWorkerLogExtra({safeCexSentryReportCooldownMs:null})});
const inherited=Object.create({safeCexSentryReportKey:'inherited',safeCexSentryReportCooldownMs:1,safeCexSentryTags:{scope:'prototype'},safeCexSentryExtra:{detail:1},sentryTags:{scope:'prototype'},sentryExtra:{detail:2},suppressConsoleLog:true});
const hidden={};Object.defineProperty(hidden,'safeCexSentryReportKey',{value:'hidden'});
const omissions=[inherited,hidden].map(input=>({input,control:h.getWorkerSentryControl(input),split:h.splitWorkerLogExtra(input)}));
const nonfiniteSuppression=[Infinity,-Infinity].map(n=>({input:{suppressConsoleLog:n},split:h.splitWorkerLogExtra({suppressConsoleLog:n})}));
const overflowJson=['1e400','-1e400','1e-400','-1e-400','0','-0'].map(number=>{const inputJson=`{"suppressConsoleLog":${number}}`;return {inputJson,split:h.splitWorkerLogExtra(JSON.parse(inputJson))};});
const reads=[];
const controlled={};
for(const [key,value] of Object.entries({safeCexSentryReportKey:'key',safeCexSentryReportCooldownMs:5,safeCexSentryTags:{t:true},safeCexSentryExtra:{e:true}}))Object.defineProperty(controlled,key,{enumerable:true,get(){reads.push(key);return value;}});
h.getWorkerSentryControl(controlled);const controlReads=reads.splice(0);
const splitInput={};
for(const [key,value] of Object.entries({sentryTags:{t:true},sentryExtra:{e:true},suppressConsoleLog:Infinity,plain:3}))Object.defineProperty(splitInput,key,{enumerable:true,get(){reads.push(key);return value;}});
h.splitWorkerLogExtra(splitInput);const splitReads=reads.splice(0);
process.stdout.write(JSON.stringify({records,omissions,nonfiniteSuppression,overflowJson,controlReads,splitReads}));
