import assert from 'node:assert/strict';
import crypto from 'node:crypto';
import fs from 'node:fs';
import vm from 'node:vm';
import {fileURLToPath} from 'node:url';
import {ts} from './source.mjs';

const filename=fileURLToPath(new URL('../../../packages/safe-cex/src/exchanges/mexc/mexc.exchange.ts',import.meta.url));
let source=fs.readFileSync(filename,'utf8');
if(process.env.MEXC_ACCOUNT_TS_CONTROL==='empty-key-nullish'){
 const before=".update(this.options.key || 'mexc')";
 assert(source.includes(before),'MEXC account key fallback mutation anchor missing');
 source=source.replace(before,".update(this.options.key ?? 'mexc')");
}
if(process.env.MEXC_ACCOUNT_TS_CONTROL==='response-detail'){
 const before='err?.response?.data?.message || err?.response?.data?.msg';
 assert(source.includes(before),'MEXC account detail mutation anchor');
 source=source.replace(before,'err?.response?.data?.msg || err?.response?.data?.message');
}
const ast=ts.createSourceFile(filename,source,ts.ScriptTarget.Latest,true);
const klass=ast.statements.find(node=>ts.isClassDeclaration(node)&&node.name?.text==='MexcExchange');
assert(klass,'MexcExchange declaration missing');
const initializer=name=>{
 const method=klass.members.find(node=>ts.isPropertyDeclaration(node)&&node.name.getText(ast)===name);
 assert(method?.initializer&&ts.isArrowFunction(method.initializer),`${name} initializer missing`);
 return `exports.${name}=async function ${name}(${method.initializer.parameters.map(p=>p.getText(ast)).join(",")}) ${method.initializer.body.getText(ast)};`;
};
const js=ts.transpileModule([initializer('validateAccount'),initializer('getAccount'),initializer('fetchMarkets'),initializer('fetchTickers'),initializer('fetchPositions')].join('\n'),{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
const module={exports:{}};
vm.runInNewContext(`(function(exports){${js}\n})(module.exports)`,{module,createHash:crypto.createHash,ENDPOINTS:{CONTRACT_DETAIL:"markets",TICKER:"tickers",OPEN_POSITIONS:"positions"}});
const balance={free:10,used:2,total:12};
const instance={options:{key:'key'},fetchBalance:async()=>balance,emitAndLogError:message=>{instance.emitted.push(message);},emitted:[]};
const validation=await module.exports.validateAccount.call(instance);
assert.equal(validation.error,'');
assert.equal(validation.balance,balance);
const account=await module.exports.getAccount.call({...instance,validateAccount:module.exports.validateAccount});
const empty=await module.exports.getAccount.call({...instance,options:{key:''},validateAccount:module.exports.validateAccount});
const failing={options:{key:'key'},fetchBalance:async()=>{const error=new Error('transport failure');error.response={data:{msg:'response detail'}};throw error;},emitAndLogError:()=>{}};
const failedValidation=await module.exports.validateAccount.call(failing);
let failedAccount;
try{await module.exports.getAccount.call({...failing,validateAccount:module.exports.validateAccount});failedAccount={error:'',name:'none'};}
catch(error){failedAccount={error:error.message,name:error.name};}
const rawInputs = [
 [{message:'first',msg:'second'},'transport'],
 [{message:'',msg:'second'},'transport'],
 [{message:false,msg:0},'transport'],
 [{},''],
 [{message:42},'transport'],
 [{message:{message:'object detail',code:313}},'transport'],
];
const rawCases=[];
for(const [data,message] of rawInputs){
 const emitted=[];
 const host={fetchBalance:async()=>{const err=new Error(message);err.response={data};throw err;},emitAndLogError:value=>emitted.push(value)};
 const result=await module.exports.validateAccount.call(host);
 rawCases.push({error:result.error,emitted});
}
const snapshotCases=[];
for(const method of ['fetchMarkets','fetchTickers','fetchPositions']){
 const detail={message:'inner detail',code:313};
 const emitted=[];
 const host={isPublicDataOnly:false,xhr:{get:async()=>{const e=new Error('transport');e.response={data:{message:detail}};throw e;}},store:{markets:[],tickers:[],positions:[]},emitAndLogError:value=>emitted.push(value)};
 const fallback=await module.exports[method].call(host);
 snapshotCases.push({method,emitted,fallbackLength:fallback.length});
}
process.stdout.write(JSON.stringify({captureArchitecture:process.arch,captureNodeMajor:Number(process.versions.node.split('.')[0]),rawCases,snapshotCases,cases:[validation,{userId:account.userId},{userId:empty.userId},failedValidation,failedAccount]}));
