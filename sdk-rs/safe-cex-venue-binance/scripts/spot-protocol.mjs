import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {createRequire} from 'node:module';
const require=createRequire(import.meta.url);
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const root=new URL('../../../',import.meta.url);
const read=file=>ts.createSourceFile(file,fs.readFileSync(new URL(file,root),'utf8'),ts.ScriptTarget.Latest,true);
const ast=read('packages/safe-cex/src/exchanges/binance/binancespot.ws-private.ts');
const types=read('packages/safe-cex/src/exchanges/binance/binancespot.types.ts');
const names=['BINANCE_SPOT_USER_DATA_REQUEST_ID','BINANCE_SPOT_HEARTBEAT_REQUEST_ID','buildBinanceSpotUserDataSignaturePayload'];
const nodes=ast.statements.filter(ts.isVariableStatement).filter(s=>s.declarationList.declarations.some(n=>names.includes(n.name.getText(ast))));
assert.equal(nodes.length,names.length);
const window=types.statements.filter(ts.isVariableStatement).find(s=>s.declarationList.declarations.some(n=>n.name.getText(types)==='USER_DATA_RECV_WINDOW'));
const exports={};
vm.runInNewContext(ts.transpileModule(window.getText(types)+'\n'+nodes.map(n=>n.getText(ast)).join('\n'),{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{exports});
const numbers=[NaN,Infinity,-Infinity,-0,0,1,-1,1.5,1e-7,1e-6,1e20,1e21,Number.MAX_VALUE,Number.MIN_VALUE];
for(let exponent=-323;exponent<=308;exponent+=7) numbers.push(1.23456789*10**exponent);
const encode=n=>n===undefined?'undefined':Object.is(n,-0)?'-0':String(n);
const cases=[];
for(const key of ['', 'key', 'a&b=c', 'has space', 'é\ufeff🦀', '"\\\n']) for(const timestamp of numbers) for(const recvWindow of [undefined,...numbers.slice(0,14)]) {
 cases.push({key,timestamp:encode(timestamp),window:encode(recvWindow),expected:exports.buildBinanceSpotUserDataSignaturePayload(key,timestamp,recvWindow)});
}
const unions={};
for(const [file,name] of [['binancepm.api.ts','ApiType'],['binance.orderbook.ts','BinanceOrderBookApplyResult']]) {
 const source=read(`packages/safe-cex/src/exchanges/binance/${file}`);
 const alias=source.statements.find(n=>ts.isTypeAliasDeclaration(n)&&n.name.text===name);
 assert.ok(alias&&ts.isUnionTypeNode(alias.type));
 unions[name]=alias.type.types.map(n=>{assert.ok(ts.isLiteralTypeNode(n)&&ts.isStringLiteral(n.literal));return n.literal.text;});
}
process.stdout.write(JSON.stringify({unions,userData:exports.BINANCE_SPOT_USER_DATA_REQUEST_ID,heartbeat:exports.BINANCE_SPOT_HEARTBEAT_REQUEST_ID,cases}));
