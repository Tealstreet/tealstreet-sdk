import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {fileURLToPath} from 'node:url';
import {load,ts} from './source.mjs';

const filename=fileURLToPath(new URL('../../../packages/safe-cex/src/exchanges/mexc/mexc.exchange.ts',import.meta.url));
let source=fs.readFileSync(filename,'utf8');
if(process.env.MEXC_FETCH_BALANCE_CONTROL==='drop-rows'){
 const anchor='return normalizeBalance(extractRows(data));';
 assert(source.includes(anchor),'fetchBalance mutation anchor missing');
 source=source.replace(anchor,'return normalizeBalance([]);');
}
const ast=ts.createSourceFile(filename,source,ts.ScriptTarget.Latest,true);
const variables=new Map();
for(const statement of ast.statements){
 if(!ts.isVariableStatement(statement))continue;
 for(const declaration of statement.declarationList.declarations){
  if(['extractData','extractRows'].includes(declaration.name.getText(ast)))variables.set(declaration.name.getText(ast),declaration);
 }
}
const klass=ast.statements.find(node=>ts.isClassDeclaration(node)&&node.name?.text==='MexcExchange');
assert(klass,'MexcExchange class not found');
const method=klass.members.find(node=>ts.isPropertyDeclaration(node)&&node.name.getText(ast)==='fetchBalance');
assert(method?.initializer&&ts.isArrowFunction(method.initializer),'fetchBalance initializer not found');
const initializer=`async function() ${method.initializer.body.getText(ast)}`;
const code=[...['extractData','extractRows'].map(name=>`const ${variables.get(name).getText(ast)}`),`exports.fetchBalance=${initializer};`].join('\n');
const js=ts.transpileModule(code,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
const module={exports:{}};
const normalizeBalance=load('packages/safe-cex/src/exchanges/mexc/mexc.utils.ts').normalizeBalance;
vm.runInNewContext(`(function(exports){${js}\n})(module.exports)`,{module,ENDPOINTS:{ASSETS:'/api/v1/private/account/assets'},normalizeBalance});
const raw=[
 {currency:'USDT',availableBalance:'10',equity:'12',unrealized:'2',frozenBalance:'1'},
 {currency:'USDC',available:'5',balance:'7',unrealizedPnl:'-1'},
 {currency:'BTC',free:'3',total:'4'},
];
const instance={xhr:{get:async url=>{assert.equal(url,'/api/v1/private/account/assets');return {data:raw};}}};
const balance=await module.exports.fetchBalance.call(instance);
const bits=value=>{if(Number.isNaN(value))return '7ff8000000000000';const bytes=Buffer.alloc(8);bytes.writeDoubleBE(value);return bytes.toString('hex');};
const view=value=>({free:bits(value.free),used:bits(value.used),total:bits(value.total),upnl:bits(value.upnl),keys:Object.keys(value.currencies),currencies:Object.fromEntries(Object.entries(value.currencies).map(([key,row])=>[key,{free:bits(row.free),used:bits(row.used),total:bits(row.total),upnl:bits(row.upnl)}]))});
const timingRows=[raw[0],null];
let timing;
try{
 const later={xhr:{get:async()=>({data:timingRows})}};
 await module.exports.fetchBalance.call(later);
 timing={name:'none',message:''};
}catch(error){timing={name:error.name,message:error.message};}
process.stdout.write(JSON.stringify({captureArchitecture:process.arch,captureNodeMajor:Number(process.versions.node.split('.')[0]),raw,expected:view(balance),alias:balance.info===raw&&balance.currencies.USDT.info===raw[0],timing}));
