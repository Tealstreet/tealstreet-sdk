import ts from 'typescript';
import vm from 'node:vm';
import { readFileSync } from 'node:fs';
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';

const root = new URL('../../../packages/safe-cex/src/exchanges/bybit/', import.meta.url);
const compilerOptions = { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS };
const helpers = { exports: {} };
vm.runInNewContext(ts.transpileModule(readFileSync(new URL('bybit.options.ts', root), 'utf8'), { compilerOptions }).outputText, helpers);
const getKey = { exports: {}, require: createRequire(import.meta.url) };
vm.runInNewContext(ts.transpileModule(readFileSync(new URL('../../utils/get-key.ts', root), 'utf8'), {
  compilerOptions: { ...compilerOptions, esModuleInterop: true },
}).outputText, getKey);
const file = ts.createSourceFile('bybit.exchange.ts', readFileSync(new URL('bybit.exchange.ts', root), 'utf8'), ts.ScriptTarget.Latest, true);
const names = ['fetchOptionChain', 'fetchOptionExpiries', 'fetchAccountGreeks', 'assertOptionsEnabled'];
const cls = file.statements.find(n => ts.isClassDeclaration(n) && n.members.some(m => m.name?.getText(file) === names[0]));
const members = names.map(name => {
  const matches = cls.members.filter(m => m.name?.getText(file) === name);
  assert.equal(matches.length, 1, name);
  return matches[0].getText(file);
}).join('\n');
const context = { ...helpers.exports, ENDPOINTS: { TICKERS: '/v5/market/tickers', COIN_GREEKS: '/v5/asset/coin-greeks' }, ...getKey.exports };
vm.runInNewContext(ts.transpileModule(`globalThis.Owner=class { ${members} }`, { compilerOptions }).outputText, context);
const market = (symbol, underlying, expiry, strike, right) => ({ id: symbol, symbol, base: underlying, quote: 'USDT', active: true,
  precision: { amount: .01, price: .1 }, limits: { amount: { min: .01, max: 100 }, leverage: { min: 1, max: 1 } },
  option: { underlying, expiry, strike, right, style: 'european', settlement: 'cash', premiumAsset: 'USDT', settleAsset: 'USDT' } });
const markets = [market('put','BTC',2000,100,'put'),market('call','BTC',2000,100,'call'),market('later','BTC',3000,50,'call'),market('earlier','BTC',1000,200,'call'),market('eth','ETH',500,10,'call')];
const encode = value => {
  if (typeof value === 'number') { const bytes=Buffer.alloc(8); bytes.writeDoubleBE(value); return { numberBits: bytes.toString('hex') }; }
  if (Array.isArray(value)) return value.map(encode);
  if (value && typeof value === 'object') return Object.fromEntries(Object.entries(value).filter(([,v])=>v!==undefined).map(([k,v])=>[k,encode(v)]));
  return value;
};
const cases = [];
async function add(method, query, list, enabled = true, response) {
  const calls = [];
  const owner = new context.Owner();
  owner.optionsEnabled = () => enabled;
  owner.store = { markets };
  owner.xhr = { async get(...args) { calls.push(args); return { data: response ?? {retCode:0,result:{list}} }; } };
  let result, error;
  try { result = encode(await owner[method](query)); } catch(e) { error={name:e.name,message:e.message}; }
  cases.push({method,query,list,enabled,response,markets,result,error,calls});
}
const quote = (symbol, value) => ({symbol, bid1Price:value, ask1Price:value, markPrice:value, markIv:value, bid1Iv:value, ask1Iv:value, underlyingPrice:value, delta:value,gamma:value,vega:value,theta:value});
for(const query of [{underlying:'BTC'},{underlying:'ETH'},{underlying:'SOL'},{underlying:'BTC',expiry:2000},{underlying:'BTC',right:'call'},
  {underlying:'BTC',right:'put'},{underlying:'BTC',strikeMin:100},{underlying:'BTC',strikeMax:100},{underlying:'BTC',strikeMin:101,strikeMax:199},
  {underlying:'BTC',strikeMin:0,strikeMax:0},{underlying:'BTC',expiry:2000,right:'call',strikeMin:100,strikeMax:100}]) {
  await add('fetchOptionChain',query,[quote('call','1'),quote('put','2'),quote('earlier','3'),quote('later','4'),quote('eth','5'),quote('call','9')]);
}
const atoms=[undefined,null,false,true,0,-0,1,'','-0','12.5tail','0x10','Infinity','-Infinity','NaN','\uFEFF12.5','\u008512.5',[],[12.5],[1,2],{}, {toString:null}];
for(const value of atoms) {
  await add('fetchOptionChain',{underlying:'BTC'},[quote('call',value)]);
  await add('fetchAccountGreeks',undefined,[{baseCoin:'BTC',totalDelta:value,totalGamma:value,totalVega:value,totalTheta:value}]);
}
for(const raw of [{delta:'0.5',gamma:'0.01',vega:'3',theta:'-4'},{delta:''},{},{delta:0,gamma:'x'},{delta:'1e309',gamma:null}]) {
  await add('fetchOptionChain',{underlying:'BTC'},[{symbol:'call',...raw}]);
  await add('fetchAccountGreeks',undefined,[{baseCoin:'BTC',totalDelta:raw.delta,totalGamma:raw.gamma,totalVega:raw.vega,totalTheta:raw.theta}]);
}
for(const method of names.slice(0,3)) {
  const query=method==='fetchOptionChain'?{underlying:'BTC'}:method==='fetchOptionExpiries'?'BTC':undefined;
  await add(method,query,[],false);
  if(method!=='fetchOptionExpiries') {
    for(const response of [{retCode:'0'}, {retCode:0,result:null},{retCode:0,result:{list:null}}, {retCode:10001,retMsg:'option refused'}]) await add(method,query,undefined,true,response);
    await add(method,query,[null]);
  }
}
for(const base of ['BTC','ETH','SOL']) await add('fetchOptionExpiries',base);
assert.equal(cases.length,79);
process.stdout.write(JSON.stringify(cases));
