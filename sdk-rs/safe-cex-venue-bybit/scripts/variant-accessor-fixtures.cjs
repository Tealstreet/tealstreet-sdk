// Execute maintained class members, including the real base subscription method.
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const assert = require('node:assert/strict');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const root = path.resolve(__dirname, '../../..');
function members(relative, className, names) {
  const source = fs.readFileSync(path.join(root, relative), 'utf8');
  const ast = ts.createSourceFile(relative, source, ts.ScriptTarget.Latest, true);
  const cls = ast.statements.find(s => ts.isClassDeclaration(s) && s.name.text === className);
  assert(cls, `source class ${className}`);
  return names.map(name => {
    const matches = cls.members.filter(m => m.name?.getText(ast) === name);
    assert.equal(matches.length, 1, `unique source member ${className}.${name}`);
    return matches[0].getText(ast);
  }).join('\n');
}
const context = vm.createContext({console:{warn(message){context.warnings.push(message);}}});
const baseMembers = members('packages/safe-cex/src/exchanges/base.ts', 'BaseExchange', ['getTickerSymbolsForSubscription','getActiveTickerSymbols']);
const configs = [
  ['inverse','bybitinverse.exchange.ts','BybitInverseExchange',['accountType','supportedCategories','getSettleCoinsForCategory','getCategoryForSymbol','getTickerSymbolsForPublicStream']],
  ['spot','bybitspot.exchange.ts','BybitSpotExchange',['accountType','getTickerSymbolsForPublicStream']],
  ['hyro','hyrotrader.exchange.ts','HyrotraderExchange',['isFiniteNumeric','supportsTradeDerivedCandles','supportedCategories','getSettleCoinsForCategory','requiresOrderLinkId']],
];
for (const [family,file,className,names] of configs) {
  const extracted = members('packages/safe-cex/src/exchanges/bybit/'+file, className, names);
  vm.runInContext(ts.transpileModule(`{ class Subject { ${baseMembers}\n${extracted}\n } globalThis.${family}=new Subject(); }`, {compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText, context);
}
function observe(value) {
  if (value === undefined) return {undefined:true};
  if (Array.isArray(value)) return Array.from(value,observe);
  return value;
}
function setup(subject, markets=[]) {
  subject.store={markets}; subject._activeTickerSymbols=[];
  subject.logs=[];
  subject.emitAndLogError = message => {
    subject.logs.push(message);
    if (subject.hostThrows) throw Object.assign(new Error('host logging failed'), {name:'HostError', marker:37});
  };
  context.warnings=[];
}
const statusTokens = [null,'null','false','true','0','-0','1','1.0','1.00001','6','2','"1"','" 1 "','[]','[1]','{}','1e309','"NaN"','-1e309'];
assert.equal(statusTokens.length,19,'VARIANT independent status floor');
const accounts=[];
for (const family of ['inverse','spot']) for (const raw of statusTokens) {
  context[family].unifiedMarginStatus = raw===null ? undefined : JSON.parse(raw);
  accounts.push({family,raw,result:context[family].accountType});
}
assert.equal(accounts.length,38,'VARIANT independent account floor');
const market=(symbol,quote)=>({id:symbol,symbol,base:'BTC',quote,active:true,precision:{amount:0.01,price:0.5},limits:{amount:{min:0,max:1000},leverage:{min:1,max:100}}});
const sets=[[],[market('BTCUSD','USD'),market('BTCUSDT','USDT'),market('ETHUSD','USD'),market('BTCUSDC','USDC'),market('BTCUSD','USD')],[market('BTCUSDT','USDT'),market('ETHUSDC','USDC')],[market('LOWER','usdt'),market('EMPTY',''),market('X','EUR')]];
const activeSets=[[],['BTCUSD'],['BTCUSDT','BTCUSD','BTCUSD'],['MISSING'],['X','LOWER']];
assert.equal(sets.length,4); assert.equal(activeSets.length,5);
const categories=[],tickers=[];
for(const markets of sets) {
  for (const symbol of [markets[0]?.symbol || 'FIRST','MISSING','']) {
    setup(context.inverse,markets);
    const result=context.inverse.getCategoryForSymbol(symbol);
    categories.push({markets,symbol,result,warnings:context.warnings.slice()});
  }
  for(const active of activeSets) for(const family of ['inverse','spot']) {
    const subject=context[family]; setup(subject,markets); subject._activeTickerSymbols=active;
    const original=subject.getTickerSymbolsForSubscription.bind(subject); const calls=[];
    subject.getTickerSymbolsForSubscription = symbols => { calls.push(symbols.slice()); return original(symbols); };
    const result=subject.getTickerSymbolsForPublicStream();
    delete subject.getTickerSymbolsForSubscription;
    tickers.push({family,markets,active,result,calls});
  }
}
assert.equal(categories.length,12,'VARIANT independent category floor');
assert.equal(tickers.length,40,'VARIANT independent ticker floor');
const settles=[];
function settle(family,category,raw,hostThrows=false) {
  const subject=context[family]; setup(subject); subject.hostThrows=hostThrows;
  subject.unifiedMarginStatus=raw===null ? undefined : JSON.parse(raw);
  let result;
  try { result={ok:observe(subject.getSettleCoinsForCategory(category))}; }
  catch(error) { result={error:error.name,message:error.message,marker:error.marker??null}; }
  settles.push({family,category,raw,hostThrows,result,logs:subject.logs.slice()});
}
for(const category of ['linear','inverse','spot','','LINEAR']) {
  for(const raw of statusTokens) settle('inverse',category,raw);
  settle('hyro',category,null);
}
for(const family of ['inverse','hyro']) for(const category of ['linear','spot']) settle(family,category,'1',true);
assert.equal(settles.length,104,'VARIANT independent settle floor');
const numericTokens = [null,'null','true','false','0','-0','3.25','-7','1e309','-1e309','""','" "','"\\uFEFF\\t12.5 "','"\\u008512.5"','"0xff"','"0b10"','"0o17"','"+0xff"','"12tail"','"Infinity"','"-Infinity"','"NaN"','"1e-999"','"-0"','"3.25"','[]','[2]','{}','{"toString":null}'];
assert.equal(numericTokens.length,29,'VARIANT independent numeric floor');
const numeric=numericTokens.map(raw=>({raw,result:context.hyro.isFiniteNumeric(raw===null ? undefined:JSON.parse(raw))}));
const inverseFirst=context.inverse.supportedCategories; inverseFirst.push('changed');
const hyroFirst=context.hyro.supportedCategories; hyroFirst.push('changed');
const configuration={inverseCategories:context.inverse.supportedCategories,hyroCategories:context.hyro.supportedCategories,hyroCandles:context.hyro.supportsTradeDerivedCandles,hyroLinkId:context.hyro.requiresOrderLinkId()};
process.stdout.write(JSON.stringify({accounts,categories,tickers,settles,numeric,configuration}));
