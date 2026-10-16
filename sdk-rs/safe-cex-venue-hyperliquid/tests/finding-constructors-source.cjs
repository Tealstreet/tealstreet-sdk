'use strict';
const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const root=path.resolve(__dirname,'../../..');
function actual(file,name){const ast=ts.createSourceFile(file,fs.readFileSync(path.join(root,file),'utf8'),ts.ScriptTarget.Latest,true);const matches=ast.statements.filter(n=>ts.isClassDeclaration(n)&&n.name.text===name);assert.equal(matches.length,1,'CONSTRUCTORS actual maintained class');return matches[0].getText(ast)}
const context={exports:{},REST_WEIGHT_LIMIT_PER_MINUTE:1200,REST_WEIGHT_WINDOW_MS:60000};
const text=actual('packages/safe-cex/src/exchanges/hyperliquid/hyperliquid.rateLimiter.ts','RateLimiter')+'\n'+actual('packages/safe-cex/src/exchanges/hyperliquid/hyperliquid.order-response.ts','HyperliquidUnreadableOrderResponseError');
vm.runInNewContext(ts.transpileModule(text,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,context);
let completed=0;
for(const [capacity,window,expected]of[[undefined,undefined,50],[600,undefined,100],[undefined,1200,1],[2,10,5],[0,undefined,Infinity],[NaN,undefined,NaN]]){const value=new context.exports.RateLimiter(capacity,window);assert(Object.is(value.msPerToken,expected),'DEFAULTS source native msPerToken');assert(Object.is(value.nextAvailableAt,0),'DEFAULTS source initial nextAvailableAt');assert(value.tail&&typeof value.tail.then==='function','DEFAULTS source initial tail');completed++}
for(const message of['','arbitrary diagnostic','雪😀','embedded\0text']){const error=new context.exports.HyperliquidUnreadableOrderResponseError(message);assert.equal(error.name,'HyperliquidUnreadableOrderResponseError','ERROR source exact native name');assert.equal(error.message,message,'ERROR source original native message');assert.equal(String(error),message?`HyperliquidUnreadableOrderResponseError: ${message}`:'HyperliquidUnreadableOrderResponseError','ERROR source Error display');completed++}
assert.equal(completed,10,'CONSTRUCTORS independent source floor');process.stdout.write(JSON.stringify({count:completed,last:completed-1}));
