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
const ast=ts.createSourceFile(filename,source,ts.ScriptTarget.Latest,true);
const klass=ast.statements.find(node=>ts.isClassDeclaration(node)&&node.name?.text==='MexcExchange');
assert(klass,'MexcExchange declaration missing');
const initializer=name=>{
 const method=klass.members.find(node=>ts.isPropertyDeclaration(node)&&node.name.getText(ast)===name);
 assert(method?.initializer&&ts.isArrowFunction(method.initializer),`${name} initializer missing`);
 return `exports.${name}=async function ${name}() ${method.initializer.body.getText(ast)};`;
};
const js=ts.transpileModule([initializer('validateAccount'),initializer('getAccount')].join('\n'),{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
const module={exports:{}};
vm.runInNewContext(`(function(exports){${js}\n})(module.exports)`,{module,createHash:crypto.createHash});
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
process.stdout.write(JSON.stringify({captureArchitecture:process.arch,captureNodeMajor:Number(process.versions.node.split('.')[0]),cases:[validation,{userId:account.userId},{userId:empty.userId},failedValidation,failedAccount]}));
