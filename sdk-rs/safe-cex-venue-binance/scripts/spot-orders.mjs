import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import {createRequire} from 'node:module';
const require=createRequire(import.meta.url),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const runtimeRequire=process.env.SAFE_CEX_TYPESCRIPT_PATH?name=>require(path.resolve(process.env.SAFE_CEX_TYPESCRIPT_PATH,'..',name)):require;
const root=new URL('../../../packages/safe-cex/src/',import.meta.url);
const read=file=>ts.createSourceFile(file,fs.readFileSync(new URL(file,root),'utf8'),ts.ScriptTarget.Latest,true);
const run=(text,ctx={})=>{const exports={};vm.runInNewContext(ts.transpileModule(text,{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{exports,...ctx});return exports;};
const types=read('types.ts'),enums=run(types.statements.filter(ts.isEnumDeclaration).map(n=>n.getText(types)).join('\n'));
const math=read('utils/safe-math.ts'),actualMath=run(math.statements.filter(n=>!ts.isImportDeclaration(n)).map(n=>n.getText(math)).join('\n'));
const {inverseObj}=run(read('utils/inverseObj.ts').text);
const {omitUndefined}=run(read('utils/omitUndefined.ts').text,{require:id=>{assert.equal(id,'lodash/omitBy');return runtimeRequire(id);}});
const idHelpers=run(read('exchanges/clientOrderId.ts').text,{require:id=>id==='../types'?enums:id==='../utils/uuid'?{uuid:()=>{throw Error('unexpected mint');}}:assert.fail(id)});
const baseAst=read('exchanges/base.ts'),baseClass=baseAst.statements.find(ts.isClassDeclaration);
const names=['resolveCallerClientOrderId','assertSingleNativeClientOrderId','wouldSplitOrder','assertCallerClientOrderIdUnsplit','rejectPreflight'];
const members=names.map(name=>{const member=baseClass.members.find(n=>n.name?.getText(baseAst)===name);assert.ok(member,name);return member.getText(baseAst);}).join('\n');
const {Base}=run('export class Base {'+members+'}',{...idHelpers,...actualMath});
const ast=read('exchanges/binance/binancespot.exchange.ts'),klass=ast.statements.find(ts.isClassDeclaration);
const tables=run(read('exchanges/binance/binance.types.ts').text,{require:id=>{assert.equal(id,'../../types');return enums;}});
let minted=0;
const context={...enums,...tables,...actualMath,...idHelpers,inverseObj,omitUndefined,omit:runtimeRequire('lodash/omit'),times:runtimeRequire('lodash/times'),uuid:()=>`uuid-${++minted}`};
const rule=klass.members.find(n=>n.name?.getText(ast)==='clientOrderIdRule');
const format=klass.members.find(n=>n.name?.getText(ast)==='formatCreateOrder');assert.ok(rule&&format);
const owner=run('exports.Owner=class extends Base {'+rule.getText(ast)+'\n'+format.getText(ast)+'}',{...context,Base}).Owner;
const encode=n=>Object.is(n,-0)?'-0':String(n);
const trials=[];
const basic={type:'limit',side:'buy',amount:3,price:100,step:0.1,max:10,market:true};
for(const type of Object.values(enums.OrderType))for(const side of ['buy','sell'])for(const tif of [undefined,...Object.values(enums.OrderTimeInForce)])for(const amount of [0,0.01,1,10,11,23,-1,NaN,Infinity])trials.push({...basic,type,side,tif,amount});
for(const amount of [1,2.3,10,11,23])for(const max of [0,-1,1,3,10,Infinity,NaN,1e-300])for(const step of [0.1,1,0,NaN,Infinity])trials.push({...basic,amount,max,step});
for(const type of Object.values(enums.OrderType))for(const price of [undefined,0,-0,1,-1,NaN,Infinity])for(const stopLoss of [undefined,0,90,NaN])for(const takeProfit of [undefined,110])trials.push({...basic,type,price,stopLoss,takeProfit});
for(const id of [undefined,'','caller','bad:spot','x'.repeat(37)])for(const type of Object.values(enums.OrderType))for(const market of [false,true])for(const amount of [0,3,23])for(const stopLoss of [undefined,0,90])trials.push({...basic,id,type,market,amount,stopLoss});
// Field-identity supplement: original trials remain unchanged; captured live on each engine.
for(const side of ['buy','sell'])for(const [priceStep,step] of [[0.1,1],[0.25,0.1]])for(const brackets of [{stopLoss:90.36},{takeProfit:110.74},{stopLoss:90.36,takeProfit:110.74}])trials.push({...basic,side,priceStep,step,...brackets,fieldIdentity:true});
const originalTrialCount=trials.length;
for(const side of ['buy','sell'])for(const [amount,step,priceStep,max] of [[3.6,1,0.1,3.8],[3.6,0.1,1,10],[5,1,0.1,1.6],[5,0.1,1,1.6]])trials.push({...basic,side,amount,step,priceStep,max,price:12.31,precisionIdentity:true});
const results=[];
for(const trial of trials) {
 const c={...trial};
 const opts={symbol:'BTCUSDT',type:c.type,side:c.side,amount:c.amount,price:c.price,timeInForce:c.tif,stopLoss:c.stopLoss,takeProfit:c.takeProfit,clientOrderId:c.id};
 const instance=new owner();instance._exchangeName='binancespot';instance.store={marketsBySymbol:c.market?{BTCUSDT:{precision:{price:c.priceStep??0.1,amount:c.step},limits:{amount:{max:c.max}}}}:{}};
 const emitted=[];instance.emitAndLogError=message=>emitted.push(message);
 minted=0;let expected;try{const value=instance.formatCreateOrder(opts);expected={value,keys:value.map(Object.keys)};}catch(e){expected={error:e.message};}
 results.push({...c,amount:encode(c.amount),price:c.price===undefined?undefined:encode(c.price),step:encode(c.step),max:encode(c.max),stopLoss:c.stopLoss===undefined?undefined:encode(c.stopLoss),takeProfit:c.takeProfit===undefined?undefined:encode(c.takeProfit),expected,minted,emitted});
}
process.stdout.write(JSON.stringify(process.argv.includes('--precision-identity')?results.slice(originalTrialCount):results.slice(0,originalTrialCount)));
