// Live bit capture: validated Node 26.7.0 on darwin/arm64; CI captures on its own host.
import fs from 'node:fs';
import vm from 'node:vm';
import {createRequire} from 'node:module';
const require=createRequire(import.meta.url),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const path=new URL('../../../packages/safe-cex/src/webworker/ExchangeWorker.ts',import.meta.url);
const ast=ts.createSourceFile(String(path),fs.readFileSync(path,'utf8'),ts.ScriptTarget.Latest,true);
const cls=ast.statements.find(s=>ts.isClassDeclaration(s)&&s.name.text==='ExchangeWorker');
const method=cls.members.find(s=>ts.isMethodDeclaration(s)&&s.name.getText(ast)==='log');
if(!method)throw Error('missing actual ExchangeWorker.log');
const levelType=method.parameters.find(p=>p.name.getText(ast)==='level').type;
if(!ts.isUnionTypeNode(levelType)||!levelType.types.every(t=>ts.isLiteralTypeNode(t)&&ts.isStringLiteral(t.literal)))throw Error('unsupported log level type');
const levels=levelType.types.map(t=>t.literal.text);
const context=vm.createContext({});
vm.runInContext(ts.transpileModule(`class Probe {${method.getText(ast)}}\nglobalThis.Probe=Probe;`,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);
const receiver=new context.Probe();
function run(exchange,account,level,message,extra,fail=false){
 receiver._exchangeName=exchange;receiver._accountName=account;
 const sent=[]; receiver._ctx={postMessage(payload){sent.push(structuredClone(payload));if(fail)throw new TypeError('post failed');}};
 let error=null;try{receiver.log(level,message,extra);}catch(e){error={name:e.name,message:e.message};}
 return {exchange,account,level,message,extra,fail,sent,error,keys:sent[0]?Object.keys(sent[0].data.context):[]};
}
const extras=[undefined,{}, {exchange:null,account:false},{account:'override-account',exchange:'override-exchange',x:1},JSON.parse('{"__proto__":{"x":1},"constructor":"own","2":2,"0":0,"01":1}')];
const rows=[];
for(const exchange of ['bybit','binance','okx'])for(const account of ['','account','🚀\t'])for(const level of levels)for(const message of ['','text','λ🚀'])for(const extra of extras)for(const fail of [false,true])rows.push(run(exchange,account,level,message,extra,fail));
const undefinedOwn=run('bybit','a','info','m',{account:undefined,detail:undefined});
const ownUndefined={...undefinedOwn,ownAccount:Object.hasOwn(undefinedOwn.sent[0].data.context,'account'),ownDetail:Object.hasOwn(undefinedOwn.sent[0].data.context,'detail')};
const inherited=Object.create({account:'inherited',x:1});inherited.own='kept';
const inheritedRow=run('bybit','a','info','m',inherited);
const lone=run('bybit',JSON.parse('"\\ud800"'),'info','m',{});
const numbers=['-0','-0.0','1e400','-1e400','1e-400'].map(token=>{
 const extra=JSON.parse(`{"value":${token}}`),r=run('bybit','a','info','m',extra);
 const n=r.sent[0].data.context.value;const b=new DataView(new ArrayBuffer(8));b.setFloat64(0,n);
 return {token,bits:b.getBigUint64(0).toString(16).padStart(16,'0')};
});
const nativeNonfinite=[['NaN',NaN],['Infinity',Infinity],['-Infinity',-Infinity]].map(([name,value])=>{
 const row=run('bybit','a','info','m',{value});
 const output=row.sent[0].data.context.value;
 return {name,sourceType:typeof output,sourceKind:Number.isNaN(output)?'NaN':String(output),jsonInput:JSON.stringify({value}),row};
});
const nestedJson='{"outer":{"2":"two","1":"one","nested":{"9":9,"3":3}},"tail":0}';
const nested=run('bybit','a','info','m',JSON.parse(nestedJson));
process.stdout.write(JSON.stringify({nativeNonfinite,nested:{inputJson:nestedJson,row:nested,wire:JSON.stringify(nested.sent[0])},rows,levels,ownUndefined,inherited:inheritedRow,lone:JSON.stringify(lone),numbers}));
