import fs from 'node:fs';
import vm from 'node:vm';
import assert from 'node:assert/strict';
import {ts} from '../../safe-cex-store/tests/source_loader.mjs';
const source=new URL('../../../packages/safe-cex/src/exchanges/okx/okx.exchange.ts',import.meta.url);
const ast=ts.createSourceFile(source.pathname,fs.readFileSync(source,'utf8'),ts.ScriptTarget.Latest,true);
const owner=ast.statements.find(node=>ts.isClassDeclaration(node)&&node.name.text==='OKXExchange');
const names=['getInstrumentTypes','isSupportedMarketInstrument','getMarketInstrumentType','getIndexTickerInstId','getMarketsForIndexTickerInstId','formatMarketSymbol'];
const members=names.map(name=>{const member=owner.members.find(node=>node.name?.getText(ast)===name);assert.ok(member,name);return member.getText(ast);});
const code=ts.transpileModule('export class Selected {'+members.join('\n')+'}',{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
const module={exports:{}};vm.runInThisContext('(function(module,exports){'+code+'\n})')(module,module.exports);
const policy=new module.exports.Selected();
const cases=[];const capture=(op,input,run)=>{try{cases.push({op,input,expected:run()});}catch(error){assert.equal(error.name,'TypeError');cases.push({op,input,error:{name:error.name,message:error.message}});}};
capture('instruments',{},()=>policy.getInstrumentTypes());
for(const instType of ['SWAP','FUTURES','SPOT','swap',null,0,[],{}])for(const ctType of ['linear','inverse','LINEAR',null,{}]){
 const input={instType,ctType};capture('supported',input,()=>policy.isSupportedMarketInstrument(input));
}
const values=[undefined,null,'SWAP','FUTURES','futures',['FUTURES'],[],false,0,{}, {toString:null}, '\uFEFFFUTURES'];
for(const infoType of values)for(const instType of values){const input={info:{instType:infoType},instType};capture('type',input,()=>policy.getMarketInstrumentType(input));}
for(const instId of ['BTC-USDT-SWAP','BTC-USD-XPERP','BTC-SWAP-SWAP','BTC-SWAP-EXTRA','-SWAP','X-SWAP\n','X-SWAP\r','X-SWAP\r\n','X-SWAP\u2028','X-SWAP\u2029','X-SWAP\uFEFF',null,0,false,['BTC-USDT-SWAP'],{toString:null}])for(const instType of ['SWAP','FUTURES',false])for(const ruleType of ['xperp','normal']){
 const input={instId,instType,ruleType};capture('symbol',input,()=>policy.formatMarketSymbol(input));
}
const market=(id,uly)=>({id,symbol:id,base:'BTC',quote:'USDT',active:true,precision:{amount:1,price:1},limits:{amount:{min:1,max:100},leverage:{min:1,max:100}},info:{uly}});
for(const id of ['BTC-USDT-SWAP','BTC-SWAP-SWAP','BTC-SWAP-EXTRA','X-SWAP\n','X-SWAP\r','X-SWAP\r\n','X-SWAP\u2028','X-SWAP\u2029','X-SWAP\uFEFF'])for(const uly of [...values,'BTC-USDT']){
 const input=market(id,uly);capture('index',input,()=>policy.getIndexTickerInstId(input));
}
for(const underlying of ['BTC-USDT','OTHER',{toString:null}]){
 const markets=[market('BTC-USDT-SWAP',''),market('BTC-USDT-XPERP',underlying),market('OTHER-SWAP','OTHER'),market('BTC-USDT-SWAP','BTC-USDT')];policy.store={markets};
 capture('markets',{markets,instId:'BTC-USDT'},()=>policy.getMarketsForIndexTickerInstId('BTC-USDT'));
}
assert.equal(cases.length,401);
process.stdout.write(JSON.stringify(cases));
