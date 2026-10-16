import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import {load,root,ts} from './source.mjs';
const filename=path.join(root,'packages/safe-cex/src/exchanges/mexc/mexc.exchange.ts');
const source=fs.readFileSync(filename,'utf8');
const ast=ts.createSourceFile(filename,source,ts.ScriptTarget.Latest,true);
const selected=[];
for(const node of ast.statements){
 if(ts.isVariableStatement(node)&&node.declarationList.declarations.some(d=>['extractData','extractRows'].includes(d.name.getText(ast))))selected.push(node.getText(ast));
 if(ts.isClassDeclaration(node)&&node.name?.text==='MexcExchange')for(const member of node.members)if(['assertCommonSuccess','assertRegularCancelSucceeded'].includes(member.name?.getText(ast)))selected.push(`this.${member.name.getText(ast)}=${member.initializer.getText(ast)};`);
}
let input=selected.join('\n');
if(process.env.MEXC_TS_CONTROL==='success-guard'){
 const anchor='payload.success !== false';if(!input.includes(anchor))throw Error('missing success guard anchor');input=input.replace(anchor,'payload.success === true');
}
const context=vm.createContext(load('packages/safe-cex/src/exchanges/mexc/mexc.utils.ts'));
vm.runInContext(ts.transpileModule(input,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText,context,{filename});
const cases=[];
const run=(name,payload,action='MEXC test')=>{try{context[name](payload,action);cases.push({name,payload,action,ok:true});}catch(e){cases.push({name,payload,action,error:{name:e.name,message:e.message}});}};
for(const payload of [undefined,null,false,0,'',[],{}])run('assertCommonSuccess',payload);
for(const code of [null,0,'0','0x10','0abc',1,-1,false,true,[],[0],{},'NaN','Infinity'])for(const success of [undefined,null,false,true,0,'false'])run('assertCommonSuccess',{code,...(success===undefined?{}:{success})});
for(const message of [null,false,0,'',1,'boom',[],[1,2],{}, {toString:null},'😃','\uFEFF'])for(const msg of [undefined,'fallback'])run('assertCommonSuccess',{code:2,message,...(msg===undefined?{}:{msg})},'');
for(const data of [[],[{}],[null],[{errorCode:0}],[{errorCode:'0abc'}],[{errorCode:1,orderId:0}],[{errorCode:2,orderId:'123',errorMsg:'bad'},{errorCode:3,orderId:'456',message:'worse'}],[{errorCode:1,orderId:{toString:null}},null],[null,{errorCode:1,orderId:{toString:null}}]])run('assertRegularCancelSucceeded',{code:0,data});
for(const value of [undefined,null,false,0,'',[],{},'x'])run('assertRegularCancelSucceeded',{code:0,data:[{errorCode:1,orderId:value,errorMsg:'specific error',msg:'fallback'}]});
process.stdout.write(JSON.stringify(cases));
