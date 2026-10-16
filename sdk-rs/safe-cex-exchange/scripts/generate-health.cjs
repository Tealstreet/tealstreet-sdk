const fs = require('node:fs');
const path = require('node:path');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || '../../../node_modules/typescript');
const source = path.resolve(__dirname, '../../../packages/safe-cex/src/health/errorPatterns.ts');
const exportsObject = {};
const compiled = ts.transpileModule(fs.readFileSync(source, 'utf8'), { compilerOptions: { module: ts.ModuleKind.CommonJS }}).outputText;
new Function('exports', compiled)(exportsObject);
const serialize = patterns => patterns.map(p => ({ category: p.category, severity: p.severity, patterns: p.patterns.map(m => typeof m === 'string' ? { text: m } : { regex: m.source, flags: m.flags }) }));
const output = JSON.stringify({ generatedFrom: 'packages/safe-cex/src/health/errorPatterns.ts', defaults: serialize(exportsObject.DEFAULT_ERROR_PATTERNS), exchanges: Object.fromEntries(Object.entries(exportsObject.EXCHANGE_SPECIFIC_PATTERNS).map(([k,v]) => [k,serialize(v)])) }, null, 2) + '\n';
const target = path.resolve(__dirname, '../src/health_patterns.json');
if (process.argv.includes('--check')) {
  if (fs.readFileSync(target, 'utf8') !== output) throw Error('health patterns drifted from TypeScript source');
} else fs.writeFileSync(target, output);
const monitorSource = path.resolve(__dirname, '../../../packages/safe-cex/src/health/ExchangeHealthMonitor.ts');
const monitorCompiled = ts.transpileModule(fs.readFileSync(monitorSource,'utf8'),{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText;
let clock=100000, nextTimer=0;const timers=new Map();const monitorExports={};
new Function('exports','require','Date','setTimeout','clearTimeout',monitorCompiled)(monitorExports,()=>exportsObject,{now:()=>clock},(fn,delay)=>{const id=++nextTimer;timers.set(id,{fn,end:clock+delay});return id;},id=>timers.delete(id));
const messages=['secret\rinvalid','secret\u2028invalid','secret\u2029invalid','market\u2028not found','rate\rlimit','timestamp\u2029ahead','ſecret invalid','api Key invalid','invalid😀api_key','retCode\u0085:10003','retCode\ufeff:10003','','order price 10005.5 out of range','Balance 33004.21 USDT insufficient','retCode=10003','retCode: 10010','retCode=33004','API key is invalid.','invalid api key','api key has expired','Remote WS signing unavailable: dropped','url is not allowed','market BTC not found invalid api key','EC_PostOnlyWillTakeLiquidity','retCode: 110125 You must agree to the Trading Terms before trading','EC_WrongParameter','missing positionId invalid api key','nothing to reduce','This trading pair does not currently support trading via OpenAPI.','recv_window','timestamp for this request is ahead of server time','order timestamp expired','service unavailable','rate limit','rate limit maintenance','Authorization header is missing','login failed, double check your key','Token invalid','wallet address','Incorrect apiKey','-2014','-1015','-1021','50111','unmatched ip','No portfolio with permissions found','session credential expired','arbitrary explicit error','order expired'];
const cases=[];
for (const exchange of ['unknown',...Object.keys(exportsObject.EXCHANGE_SPECIFIC_PATTERNS)]) {
 for (const message of messages) {clock=100000;timers.clear();const monitor=new monitorExports.ExchangeHealthMonitor(exchange);const category=monitor.processMessage(message,true);cases.push({exchange,message,category,state:monitor.getState(),summary:monitor.getSummary()});monitor.dispose();}
}
const oracleTarget=path.resolve(__dirname,'../tests/health_oracle.json');const oracleText=JSON.stringify({generatedFrom:'packages/safe-cex/src/health/ExchangeHealthMonitor.ts',cases},null,2)+'\n';
if(process.argv.includes('--check')){if(fs.readFileSync(oracleTarget,'utf8')!==oracleText)throw Error('health oracle drifted from TypeScript source');}else fs.writeFileSync(oracleTarget,oracleText);
