import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {createRequire} from 'node:module';
const require=createRequire(import.meta.url);
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const root=new URL('../../../',import.meta.url);
const read=file=>ts.createSourceFile(file,fs.readFileSync(new URL(file,root),'utf8'),ts.ScriptTarget.Latest,true);
const shared=read('packages/safe-cex/src/types.ts');
const enums={};
vm.runInNewContext(ts.transpileModule(shared.statements.filter(ts.isEnumDeclaration).map(n=>n.getText(shared)).join('\n'),{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{exports:enums});
function evaluate(text) {const exports={};vm.runInNewContext(ts.transpileModule(`exports.fn = ${text}`,{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{exports,...enums});return exports.fn;}
const result=[];
for(const variant of ['binance','binancepm']) {
 const ast=read(`packages/safe-cex/src/exchanges/binance/${variant}.exchange.ts`);
 const klass=ast.statements.find(ts.isClassDeclaration);
 const member=name=>{const n=klass.members.find(n=>n.name?.getText(ast)===name);assert.ok(n,name);return evaluate(n.initializer?.getText(ast)??`function(${n.parameters.map(p=>p.getText(ast)).join(',')}) ${n.body.getText(ast)}`);};
 // Arrow functions read lexical this. Build each real source initializer inside a bound factory.
 const n=klass.members.find(n=>n.name?.getText(ast)==='getOrderPositionSide');
 const getSide=evaluate(`function(opts) { return (${n.initializer.getText(ast)})(opts); }`);
 for(const isHedged of [false,true]) for(const type of Object.values(enums.OrderType)) for(const side of Object.values(enums.OrderSide)) for(const reduceOnly of [undefined,false,true]) {
  const input={symbol:'BTCUSDT',type,side,amount:1,reduceOnly};
  result.push({kind:'position',variant,isHedged,input,expected:getSide.call({store:{options:{isHedged}}},input)});
 }
 if(variant==='binance') {
  const algo=member('isAlgoOrderType');
  for(const type of Object.values(enums.OrderType))result.push({kind:'algo',input:type,expected:algo(type)});
  const declaration=ast.statements.filter(ts.isVariableStatement).flatMap(s=>s.declarationList.declarations).find(n=>n.name.getText(ast)==='isBinancePositionModeAlreadySetError');
  const classify=evaluate(declaration.initializer.getText(ast));
  const codes=[undefined,null,false,true,-4059,'-4059',-4059.1,'-4059.0',' -4059','\ufeff-4059',0,[],{},[-4059],{valueOf:-4059}];
  for(const code of codes) for(const input of [{response:{data:{code}}},{code},{response:{code}},{response:{data:code}}]) result.push({kind:'error',input,expected:classify(input)});
  const detect=member('detectBalanceIssue');
  for(const input of [undefined,null,false,0,[],{}, {availableBalance:0,assets:[{asset:'USDT',walletBalance:'1000'}]}]) result.push({kind:'balance',input,expected:detect(input)});
 }
}
process.stdout.write(JSON.stringify(result));
