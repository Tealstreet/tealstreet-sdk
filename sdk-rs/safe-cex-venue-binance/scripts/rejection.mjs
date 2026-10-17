import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {createRequire} from 'node:module';
const require=createRequire(import.meta.url);
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const root=new URL('../../../',import.meta.url);
const read=file=>ts.createSourceFile(file,fs.readFileSync(new URL(file,root),'utf8'),ts.ScriptTarget.Latest,true);
const shared=read('packages/safe-cex/src/types.ts');
const enums={};vm.runInNewContext(ts.transpileModule(shared.statements.filter(ts.isEnumDeclaration).map(n=>n.getText(shared)).join('\n'),{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{exports:enums});
const functions={};
for(const [file,name] of [['binance.exchange.ts','isPostOnlyRejectionError']]) {
 const ast=read(`packages/safe-cex/src/exchanges/binance/${file}`);
 const klass=ast.statements.find(ts.isClassDeclaration);const member=klass.members.find(m=>m.name?.getText(ast)===name);assert.ok(member);
 const exports={};vm.runInNewContext(ts.transpileModule(`exports.fn=${member.initializer.getText(ast)}`,{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{exports,...enums});functions[name]=exports.fn;
}
const values=[undefined,null,false,true,0,-0,1,-5022,'-5022','',[],{},['MAKER'],{toString:null},[{toString:null}],{valueOf:0},'GTX','maker','fooPOST_ONLYbar','post only order will be rejected','COULD NOT BE EXECUTED AS MAKER','PostOnly','POST-ONLY','𝕄AKER'];
const cases=[];
function add(name,input) {let expected;try{expected={value:functions[name](input)??null};}catch(e){expected={error:e.name};}cases.push({name,input,expected});}
for(const code of values)for(const msg of values)for(const message of [undefined,'could not be executed as maker',{toString:null}]) {
 for(const input of [{response:{data:{code,msg}},code:-5022,message},{code,message},{response:{data:{code,msg}},message}])add('isPostOnlyRejectionError',input);
}
for(const input of [undefined,null,false,0,'',[],{}])add('isPostOnlyRejectionError',input);
process.stdout.write(JSON.stringify(cases));
