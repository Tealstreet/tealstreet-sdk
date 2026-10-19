import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {createHash} from 'node:crypto';
import {load, root, ts} from '../tests/source.mjs';
const dir=root+'packages/safe-cex/src/exchanges/mexc/';
const utils=load('packages/safe-cex/src/exchanges/mexc/mexc.utils.ts');
const types=load('packages/safe-cex/src/exchanges/mexc/mexc.types.ts');
const {DefaultStore}=load('packages/safe-cex/src/store/store.base.ts');
const {LogSeverity}=load('packages/logging/src/LogSeverity.ts');
const sources=[];
function members(file, ownerName, names) {
 const source=fs.readFileSync(dir+file,'utf8');
 const ast=ts.createSourceFile(file,source,ts.ScriptTarget.Latest,true);
 const owners=ast.statements.filter(n=>ts.isClassDeclaration(n)&&n.name?.text===ownerName);
 assert.equal(owners.length,1);
 const selected=names.map(name=>{const rows=owners[0].members.filter(n=>n.name?.getText(ast)===name);assert.equal(rows.length,1,name);let body=rows[0].getText(ast); if (name==='listenOrderBook' && process.env.MEXC_BOOK_SOURCE_CONTROL==='defer') { const before='this.parent\n      .fetchOrderBook(symbol, 100)'; assert.ok(body.includes(before),'source timing control anchor'); body=body.replace(before,'Promise.resolve().then(() => this.parent.fetchOrderBook(symbol, 100))'); } return body;});
 sources.push({file,sha256:createHash('sha256').update(source).digest('hex'),names});
 return {source,selected};
}
const parent=members('mexc.exchange.ts','MexcExchange',['fetchOrderBook']);
const socket=members('mexc.ws-public.ts','MexcPublicWebsocket',['messageHandlers','subscriptions','orderbooks','listenOrderBook','addHandler','removeHandler','subscribe','unsubscribe']);
const extract=parent.source.slice(parent.source.indexOf('const extractData ='),parent.source.indexOf('\n\ntype MexcSymbolConfig'));
assert.ok(extract.startsWith('const extractData ='));
const socketAst=ts.createSourceFile('mexc.ws-public.ts',socket.source,ts.ScriptTarget.Latest,true);
const keyDeclarations=socketAst.statements.filter(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>d.name.getText(socketAst)==='subscriptionKey'));
assert.equal(keyDeclarations.length,1);
const key=keyDeclarations[0].getText(socketAst);
const context={...utils,ENDPOINTS:types.ENDPOINTS,LogSeverity};
const code=`${key}\n${extract}\nclass Parent { ${parent.selected.join('\n')} }\nclass Socket { ${socket.selected.join('\n')} }\nglobalThis.Parent=Parent;globalThis.Socket=Socket;`;
const compiled=ts.transpileModule(code,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS},reportDiagnostics:true});
assert.equal((compiled.diagnostics??[]).filter(d=>d.category===ts.DiagnosticCategory.Error).length,0);
vm.runInNewContext(compiled.outputText,context);
const cases=[];
for (const mode of ['held','cleanup-before-response','dispose-before-response']) {
 const store=new DefaultStore();
 store.update({markets:[utils.normalizeMarket({symbol:'BTC_USDT',contractSize:.5,maxLeverage:20})]});
 let release;const pending=new Promise(yes=>{release=yes;});
 const requests=[],books=[],logs=[];const p=new context.Parent();
 Object.assign(p,{store,xhr:{get(url,config){requests.push({url,config});return pending;}},log(...args){logs.push(args);}});
 const s=new context.Socket();Object.assign(s,{parent:p,store,isDisposed:false,isConnected:false,cleanupMessageHandlerStats(){}});
 const cleanup=s.listenOrderBook('BTCUSDT',book=>books.push(book));
 assert.equal(requests.length,1,'maintained source starts snapshot request before listen returns');
 assert.equal(books.length,0,'snapshot callback awaits reaction');
 assert.equal(requests[0].url,types.ENDPOINTS.DEPTH+'/BTC_USDT');
 assert.equal(requests[0].config.params.limit,100);
 if(mode==='cleanup-before-response')cleanup();
 if(mode==='dispose-before-response')s.isDisposed=true;
 release({data:{data:{bids:[[29,6]],asks:[[31,4]],version:9}}});
 await new Promise(setImmediate);
 assert.equal(books.length,mode==='dispose-before-response'?0:1);
 if(books.length){assert.equal(books[0].bids[0].price,29);assert.equal(books[0].bids[0].amount,3);}
 if(mode==='cleanup-before-response')assert.equal(s.subscriptions.size,1,'source deliberately re-subscribes after early cleanup');
 assert.equal(logs.length,0);
 cases.push({mode,requests:requests.length,callbacks:books.length,subscriptions:s.subscriptions.size,logs});
 if(mode!=='cleanup-before-response')cleanup();
}
process.stdout.write(JSON.stringify({sources,cases}));
