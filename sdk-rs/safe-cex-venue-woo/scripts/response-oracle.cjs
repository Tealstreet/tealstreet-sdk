const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript'),runtime=require('./oracle-runtime.cjs'),root=path.resolve(__dirname,'../../..');
const names=['parseFiniteFundingNumber','getWooResponseMessage','normalizeTimestampSeconds','normalizeTimestampMilliseconds','normalizeTransactionSymbol','normalizeRequestedSymbol','getTransactionHistorySymbolCandidates','parseTransactionRowsFromResponse'];
function execute(source,context={}){const exports={};vm.runInNewContext(ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022,esModuleInterop:true}}).outputText,{exports,...context});return exports;}
const utils=execute(fs.readFileSync(path.join(root,'packages/safe-cex/src/exchanges/woo/woo.utils.ts'),'utf8'),{require:name=>{assert.equal(name,'lodash/memoize');return runtime(name);}});
const source=fs.readFileSync(path.join(root,'packages/safe-cex/src/exchanges/woo/woo.exchange.ts'),'utf8'),ast=ts.createSourceFile('woo.ts',source,ts.ScriptTarget.Latest,true);
const klass=ast.statements.find(s=>ts.isClassDeclaration(s)&&s.name.text==='WOOXExchange');assert.ok(klass);
const members=klass.members.filter(m=>m.name&&names.includes(m.name.getText(ast)));assert.equal(members.length,names.length);
const {instance, intrinsics}=execute('class Projection {\n'+members.map(m=>m.getText(ast)).join('\n')+'\n}\nexport const instance=new Projection(); export const intrinsics={String};',utils);
if (process.argv.includes('--audit')) {
  const observed = [];
  const record = (name, value, expected) => {
    assert.deepEqual(value, expected, `WOO-RESPONSE-AUDIT ${name}`);
    observed.push({name, value});
  };
  const normalizeCache = utils.normalizeSymbol.cache;
  const reverseCache = utils.reverseSymbol.cache;
  const originalNormalizer = instance.normalizeTransactionSymbol;
  const originalTrim = intrinsics.String.prototype.trim;
  try {
    normalizeCache.set('PERP_BTC_USDT', 'injected-normalize');
    reverseCache.set('BTCUSDT', 'injected-reverse');
    record('normalizeSymbol-cache', utils.normalizeSymbol('PERP_BTC_USDT'), 'injected-normalize');
    record('reverseSymbol-cache', utils.reverseSymbol('BTCUSDT'), 'injected-reverse');
    record('transaction-cache', instance.normalizeTransactionSymbol('PERP/BTC/USDT'), 'injected-normalize');
    record('requested-cache', instance.normalizeRequestedSymbol('PERP_BTC_USDT.P'), 'injected-normalize');
    record('candidates-cache', Array.from(instance.getTransactionHistorySymbolCandidates('BTCUSDT')),
      ['injected-reverse', 'BTC_USDT_PERP', 'BTC-USDT-PERP', 'BTCUSDT']);
    normalizeCache.delete('PERP_BTC_USDT');
    record('normalizeSymbol-cache-delete', utils.normalizeSymbol('PERP_BTC_USDT'), 'BTCUSDT');
    record('transaction-cache-delete', instance.normalizeTransactionSymbol('PERP/BTC/USDT'), 'BTCUSDT');
    record('requested-cache-delete', instance.normalizeRequestedSymbol('PERP_BTC_USDT.P'), 'BTCUSDT');
    reverseCache.delete('BTCUSDT');
    record('reverseSymbol-cache-delete', utils.reverseSymbol('BTCUSDT'), 'PERP_BTC_USDT');
    record('candidates-cache-delete', Array.from(instance.getTransactionHistorySymbolCandidates('BTCUSDT')),
      ['PERP_BTC_USDT', 'BTC_USDT_PERP', 'BTC-USDT-PERP', 'BTCUSDT']);
    const events = [];
    const hooked = {[Symbol.toPrimitive](hint) { events.push(hint); return 1234.75; }};
    record('funding-object', instance.parseFiniteFundingNumber(hooked), undefined);
    record('funding-no-coercion', events.length, 0);
    record('seconds-hook', instance.normalizeTimestampSeconds(hooked), 1234);
    record('milliseconds-hook', instance.normalizeTimestampMilliseconds(hooked), 1234750);
    record('hook-order', events, ['number', 'number']);
    const sentinel = {tag: 'original-coercion-error'};
    let caught;
    try { instance.normalizeTimestampSeconds({[Symbol.toPrimitive]() { throw sentinel; }}); }
    catch (error) { caught = error; }
    record('hook-error-identity', caught === sentinel, true);
    instance.normalizeTransactionSymbol = raw => ({original: raw});
    const overridden = instance.normalizeRequestedSymbol('x:btcusdt.P');
    record('requested-live-method', overridden.original, 'BTCUSDT');
    record('requested-dynamic-result', typeof overridden, 'object');
    record('funding-native-nan', instance.parseFiniteFundingNumber(NaN), undefined);
    record('seconds-native-nan', instance.normalizeTimestampSeconds(NaN), undefined);
    record('milliseconds-native-nan', instance.normalizeTimestampMilliseconds(NaN), undefined);
    intrinsics.String.prototype.trim = () => 'not-blank';
    record('funding-overridden-trim', instance.parseFiniteFundingNumber(' '), 0);
    instance.normalizeTransactionSymbol = () => { throw sentinel; };
    caught = undefined;
    try { instance.normalizeRequestedSymbol('btc.p'); } catch (error) { caught = error; }
    record('requested-method-error-identity', caught === sentinel, true);
    const calls = [];
    instance.normalizeTransactionSymbol = raw => { calls.push(raw); };
    for (const raw of [undefined, '', 'PERP']) instance.normalizeRequestedSymbol(raw);
    record('requested-empty-suffix', calls, ['']);
  } finally {
    normalizeCache.clear();
    reverseCache.clear();
    instance.normalizeTransactionSymbol = originalNormalizer;
    intrinsics.String.prototype.trim = originalTrim;
  }
  process.stdout.write(JSON.stringify(observed));
  process.exit(0);
}
function encode(v){if(v===undefined)return {$undefined:true};if(typeof v==='number'&&(!Number.isFinite(v)||Object.is(v,-0)))return {$number:Object.is(v,-0)?'-0':String(v)};if(Array.isArray(v))return v.map(encode);if(v&&typeof v==='object')return Object.fromEntries(Object.entries(v).map(([k,x])=>[k,encode(x)]));return v;}
const cases=[];function capture(name,...args){try{cases.push({name,args:encode(args),ok:encode(instance[name](...args))});}catch(e){cases.push({name,args:encode(args),error:e.message,errorName:e.name});}}
const values=[undefined,null,false,true,0,-0,-1,0.001,999999999999,1000000000000,1000000000001,1e308,NaN,Infinity,'',' ','\ufeff','\u0085','\ufeff12\ufeff','\u008512','0x10','0b11','0o17','12abc','1e400','1e308',[],[2],[1,2],{}, {toString:null}];
for(const name of ['parseFiniteFundingNumber','normalizeTimestampSeconds','normalizeTimestampMilliseconds','normalizeTransactionSymbol'])for(const value of values)capture(name,value);
const symbols=['','PERP_BTC_USDT','PERP/BTC/USDT','BTC_USDT_PERP','BTC-USDT-PERP','BTC/USDT','x:btcperp','x:','btc.p','SPOT_ETH_USDT','BTCUSDT','USDT','ß','\ufeff','\u0085','constructor','__proto__'];
for(const symbol of symbols)for(const name of ['normalizeTransactionSymbol','normalizeRequestedSymbol','getTransactionHistorySymbolCandidates'])capture(name,symbol);
for(const message of values)for(const msg of [undefined,'secondary',' \ufeff '])for(const meta of [undefined,{message:'meta'}, {message:'\u0085'}])capture('getWooResponseMessage',{message,msg,meta},'fallback');
capture('getWooResponseMessage',{message:'  keep padding  ',msg:'secondary'},'fallback');
for(const value of [undefined,null,false,0,'text',{},[],[null,3,'fill',{id:1}]]) {capture('parseTransactionRowsFromResponse',value);capture('parseTransactionRowsFromResponse',{data:{rows:value},rows:['root']});capture('parseTransactionRowsFromResponse',{data:value,rows:[]});capture('parseTransactionRowsFromResponse',{data:value});}
const file=path.join(__dirname,'../tests/fixtures/response.json'),bytes=JSON.stringify(cases,null,2)+'\n';if(process.argv.includes('--check'))assert.equal(fs.readFileSync(file,'utf8'),bytes,'WOO response helper source drift');else fs.writeFileSync(file,bytes);console.log(`WOO response helper oracle: ${cases.length} cases`);
