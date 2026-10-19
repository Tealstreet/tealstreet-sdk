// Run the actual maintained public-book subscription; only HTTP, timers and parent I/O are supplied.
const fs = require('node:fs'), path = require('node:path'), assert = require('node:assert/strict');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || path.resolve(__dirname, '../../../node_modules/typescript'));
const {enhanceAxiosError} = require('../../safe-cex-exchange/tests/http-error-source.cjs');
const file = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/phemex/phemex.ws-public.ts');
const ast = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
const declaration = ast.statements.find(ts.isClassDeclaration);
const code = ts.transpileModule(declaration.getText(ast).replace(/^export /, '')+'\nreturn BinancePublicWebsocket;', {compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.None}}).outputText;
const constantsFile = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/phemex/phemex.types.ts');
const constantsAst = ts.createSourceFile(constantsFile, fs.readFileSync(constantsFile, 'utf8'), ts.ScriptTarget.Latest, true);
const endpointsDeclaration = constantsAst.statements.find(node => ts.isVariableStatement(node) && node.declarationList.declarations.some(declaration => declaration.name.getText(constantsAst) === 'ENDPOINTS'));
assert(endpointsDeclaration, 'actual ENDPOINTS declaration');
const constants = {}; new Function('exports', ts.transpileModule(endpointsDeclaration.getText(constantsAst), {compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText)(constants);
async function run(error) {
  let clock = 0, attempts = 0; const logs = [];
  class Base { constructor() { this.isConnected = true; this.orderBookInnerState = {}; this.parent = {xhr:{get:async () => {attempts++; throw error;}},log:(...args)=>logs.push(args)}; } send(){} cleanupMessageHandlerStats(){} }
  const Public = new Function('BaseWebSocket','nanoid','Date','LogSeverity','ENDPOINTS',code)(Base,()=> 'book',{now:()=>clock},{Warning:'warning'},constants.ENDPOINTS);
  const ws = new Public(); const cleanup = ws.listenOrderBook('BTCUSDT',()=>{throw Error('failure must not publish book');});
  for (let i=0;i<3;i++) { if(i===1)clock=1000; if(i===2)clock=31000; ws.messageHandlers['orderbook_p.BTCUSDT'][0]({sequence:101,bids:[],asks:[]}); for(let j=0;j<8;j++)await Promise.resolve(); }
  assert.equal(attempts,3); assert.equal(logs.length,2); cleanup(); return logs;
}
(async()=>{const text='unscripted request GET /md/v2/fullbook'; const error=await run(new Error(text)); const enhanced=await run(enhanceAxiosError(new Error(text),'phemex')); const plain=await run({message:'must not read this'}); const string=await run('plain failure'); const prefix='[Phemex] Orderbook snapshot request failed for BTCUSDT; waiting for next delta to retry: '; assert.equal(error[0][0],prefix+text); assert.equal(enhanced[0][0],prefix+'[phemex] Request setup error: '+text); assert.equal(plain[0][0],prefix+'[object Object]'); assert.equal(string[0][0],prefix+'plain failure'); process.stdout.write(JSON.stringify({error,enhanced,plain,string}));})().catch(error=>{console.error(error);process.exitCode=1;});
