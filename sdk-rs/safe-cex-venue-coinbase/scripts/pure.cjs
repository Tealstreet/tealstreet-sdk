const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH ?? 'typescript');
const root = path.resolve(__dirname, '../../..');
const read = p => fs.readFileSync(path.join(root,p),'utf8');
const parse = p => ts.createSourceFile(p,read(p),ts.ScriptTarget.Latest,true);
const names = ['COINBASE_USDC_ALIAS_EXCEPTIONS', 'normalizeCoinbaseUsdEquivalentProductId',
  'getCoinbaseUsdEquivalentAliasProductId', 'resolveCoinbaseUsdEquivalentMarket', 'formatOrderState', 'formatOrderType'];
const source = parse('packages/safe-cex/src/exchanges/coinbase/coinbase.utils.ts');
const selected = source.statements.filter(s => ts.isVariableStatement(s) && s.declarationList.declarations.some(d => names.includes(d.name.getText(source))));
assert.equal(selected.length,names.length,'all pure Coinbase declarations must exist');
const enums = parse('packages/safe-cex/src/types.ts').statements.filter(ts.isEnumDeclaration).map(n=>n.getText()).join('\n');
let logs=[];
const exportsObject={};
vm.runInNewContext(ts.transpileModule(enums+'\n'+selected.map(n=>n.getText(source)).join('\n'), {
  compilerOptions:{module:ts.ModuleKind.CommonJS},
}).outputText,{exports:exportsObject,console:{error:(...args)=>logs.push(args)}});
const fn=exportsObject;
const ids=['BTC-USD','BTC-USDC','ETH-USDC','USDT-USDC','EURC-USDC','USDT-USD','EURC-USD','BTC-USDT','BTC-PERP-INTX','', '-USD','-USDC','btc-usdc','💰-USDC','BTC-USDC\n','\uFEFFBTC-USDC','constructor','__proto__','toString','valueOf','hasOwnProperty'];
const aliases=ids.map(id=>({id,normal:fn.normalizeCoinbaseUsdEquivalentProductId(id),alias:fn.getCoinbaseUsdEquivalentAliasProductId(id)??null}));
const describe=v=>v===undefined?{kind:'undefined'}:typeof v==='function'?{kind:'function',name:v.name}:{kind:'value',value:v};
const markets=[];
for(const id of ids){
 const alias=fn.getCoinbaseUsdEquivalentAliasProductId(id);
 for(const exact of [undefined,null,false,0,'',[],{}, {symbol:'EXACT'}]){
  for(const other of [undefined,null,false,0,'',[],{}, {symbol:'ALIAS'}]){
   const pairs=[];
   if(exact!==undefined)pairs.push([id,exact]);
   if(alias!==undefined && other!==undefined)pairs.push([alias,other]);
   // Define own __proto__ when requested; a plain assignment would invoke its setter.
   const map=JSON.parse(JSON.stringify(Object.fromEntries(pairs)));
   markets.push({id,map,result:describe(fn.resolveCoinbaseUsdEquivalentMarket(id,map))});
  }
 }
}
const states=['PENDING','OPEN','FILLED','CANCELLED','EXPIRED','FAILED','CANCEL_QUEUED','QUEUED','','constructor','open'];
const types=['Market','Limit','Stop Limit','STOP_LIMIT','', 'market','constructor'];
const capture=(name,values)=>values.map(value=>{logs=[];const result=fn[name](value);return {value,result,logs};});
process.stdout.write(JSON.stringify({aliases,markets,states:capture('formatOrderState',states),types:capture('formatOrderType',types)}));
