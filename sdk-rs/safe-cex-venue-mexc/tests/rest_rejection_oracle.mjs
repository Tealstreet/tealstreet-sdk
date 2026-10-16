import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {root, ts, load} from './source.mjs';
const file=root+'packages/safe-cex/src/exchanges/mexc/mexc.exchange.ts';
const source=fs.readFileSync(file,'utf8');
const ast=ts.createSourceFile(file,source,ts.ScriptTarget.Latest,true);
const klass=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text==='MexcExchange');
const methods=['fetchBalance','fetchPositionMode'].map(name=>{
 const member=klass.members.find(n=>n.name?.getText(ast)===name);
 assert(member,`actual method ${name}`);return member.getText(ast);
});
const context={ENDPOINTS:load('packages/safe-cex/src/exchanges/mexc/mexc.types.ts').ENDPOINTS};
vm.runInNewContext(ts.transpileModule('class Holder{'+methods.join('\n')+'} globalThis.Holder=Holder;',
 {compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);
const result=[];
for(const [method,url] of [['fetchBalance','/api/v1/private/account/assets'],['fetchPositionMode','/api/v1/private/position/position_mode']]){
 const holder=new context.Holder();const calls=[];
 const error=Object.assign(new Error('transport failure'),{code:'ERR_NETWORK',response:{status:401,statusText:'Unauthorized',headers:{},data:{message:'response detail',marker:7}},context:{phase:'native-origin'}});
 holder.xhr={get:async url=>{calls.push(url);throw error;}};
 const pending=holder[method]();
 const prefixCalls=calls.length;
 assert.equal(prefixCalls,1,`SOURCE eager GET prefix ${method}`);
 let returned;try{await pending;}catch(e){returned=e;}
 assert.equal(returned,error,`SOURCE same original error ${method}`);
 assert.equal(returned.message,'transport failure',`SOURCE original message ${method}`);
 assert.equal(returned.code,'ERR_NETWORK',`SOURCE code ${method}`);
 assert.equal(returned.response.status,401,`SOURCE native response ${method}`);
 assert.deepEqual(calls,[url],`SOURCE independent request calls ${method}`);
 // The caller retains an Error reference. Keep this concrete ownership cost visible.
 error.response.data.marker=42;
 assert.equal(returned.response.data.marker,42,`SOURCE live original response alias ${method}`);
 result.push({method,url,message:returned.message,code:returned.code,status:returned.response.status,marker:returned.response.data.marker,prefixCalls,calls:calls.length});
}
assert.equal(result.length,2,'SOURCE two actual method rejections');
console.log(JSON.stringify(result));
