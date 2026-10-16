import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {fileURLToPath} from 'node:url';
import {load,ts} from './source.mjs';

const filename=fileURLToPath(new URL('../../../packages/safe-cex/src/exchanges/mexc/mexc.exchange.ts',import.meta.url));
let source=fs.readFileSync(filename,'utf8');
if(process.env.MEXC_POSITION_MODE_CONTROL==='force-hedged'){
 const anchor='return normalizePositionMode(extractData(data));';
 assert(source.includes(anchor),'fetchPositionMode mutation anchor missing');
 source=source.replace(anchor,'return PositionMode.Hedged;');
}
const ast=ts.createSourceFile(filename,source,ts.ScriptTarget.Latest,true);
const variables=new Map();
for(const statement of ast.statements){
 if(!ts.isVariableStatement(statement))continue;
 for(const declaration of statement.declarationList.declarations){
  if(['extractData'].includes(declaration.name.getText(ast)))variables.set(declaration.name.getText(ast),declaration);
 }
}
const klass=ast.statements.find(node=>ts.isClassDeclaration(node)&&node.name?.text==='MexcExchange');
assert(klass,'MexcExchange class not found');
const method=klass.members.find(node=>ts.isPropertyDeclaration(node)&&node.name.getText(ast)==='fetchPositionMode');
assert(method?.initializer&&ts.isArrowFunction(method.initializer),'fetchPositionMode initializer not found');
const initializer=`async function() ${method.initializer.body.getText(ast)}`;
const code=`const ${variables.get('extractData').getText(ast)}\nexports.fetchPositionMode=${initializer};`;
const js=ts.transpileModule(code,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
const module={exports:{}};
const normalizePositionMode=load('packages/safe-cex/src/exchanges/mexc/mexc.utils.ts').normalizePositionMode;
vm.runInNewContext(`(function(exports){${js}\n})(module.exports)`,{module,ENDPOINTS:{POSITION_MODE:'/api/v1/private/position/position_mode'},normalizePositionMode,PositionMode:{Hedged:'hedged'}});
const payloads=[1,2,0,'one-way','one','hedged',null,{data:null},'constructor'];
const output=[];
for(const payload of payloads){
 const instance={xhr:{get:async url=>{assert.equal(url,'/api/v1/private/position/position_mode');return {data:payload};}}};
 const mode=await module.exports.fetchPositionMode.call(instance);
 output.push({captureArchitecture:process.arch,captureNodeMajor:Number(process.versions.node.split('.')[0]),payload,mode});
}
process.stdout.write(JSON.stringify(output));
