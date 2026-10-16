import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {createRequire} from 'node:module';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {load,ts} from './source.mjs';

const filename=fileURLToPath(new URL('../../../packages/safe-cex/src/exchanges/mexc/mexc.exchange.ts',import.meta.url));
let source=fs.readFileSync(filename,'utf8');
if(process.env.MEXC_FETCH_POSITIONS_CONTROL==='omit-synthetic'){
 const anchor="return uniqBy([...openPositions, ...fakePositions], 'id');";
 assert(source.includes(anchor),'fetchPositions mutation anchor missing');
 source=source.replace(anchor,'return openPositions;');
}
const ast=ts.createSourceFile(filename,source,ts.ScriptTarget.Latest,true);
const variables=new Map();
for(const statement of ast.statements){
 if(!ts.isVariableStatement(statement))continue;
 for(const declaration of statement.declarationList.declarations){
  if(['extractData','extractRows'].includes(declaration.name.getText(ast)))variables.set(declaration.name.getText(ast),declaration);
 }
}
const klass=ast.statements.find(node=>ts.isClassDeclaration(node)&&node.name?.text==='MexcExchange');
assert(klass,'MexcExchange class not found');
const method=klass.members.find(node=>ts.isPropertyDeclaration(node)&&node.name.getText(ast)==='fetchPositions');
assert(method?.initializer&&ts.isArrowFunction(method.initializer),'fetchPositions initializer not found');
const code=[...['extractData','extractRows'].map(name=>`const ${variables.get(name).getText(ast)}`),`exports.fetchPositions=async function(symbols) ${method.initializer.body.getText(ast)};`].join('\n');
const js=ts.transpileModule(code,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
const module={exports:{}};
const utils=load('packages/safe-cex/src/exchanges/mexc/mexc.utils.ts');
const positionTypes={PositionSide:{Long:'long',Short:'short'},PositionMode:{Hedged:'hedged',OneWay:'oneway'},MarginType:{Cross:'cross',Isolated:'isolated'}};
const require=createRequire(path.join(path.resolve(process.env.SAFE_CEX_TYPESCRIPT_PATH),'../mexc-oracle.cjs'));
if(process.env.MEXC_FETCH_POSITIONS_CONTROL==='missing-lodash')require('lodash/mexc-oracle-missing-entry');
const uniqBy=require('lodash/uniqBy');
vm.runInNewContext(`(function(exports){${js}\n})(module.exports)`,{module,ENDPOINTS:{OPEN_POSITIONS:'/api/v1/private/position/open_positions'},normalizePosition:utils.normalizePosition,fromMexcSymbol:utils.fromMexcSymbol,fakePositionsForMarket:utils.fakePositionsForMarket,uniqBy,...positionTypes});
const bits=value=>{if(Number.isNaN(value))return '7ff8000000000000';const bytes=Buffer.alloc(8);bytes.writeDoubleBE(value);return bytes.toString('hex');};
const view=positions=>positions.map(p=>({id:p.id,symbol:p.symbol,side:p.side,entryPrice:bits(p.entryPrice),notional:bits(p.notional),leverage:bits(p.leverage),marginType:p.marginType,positionMode:p.positionMode,unrealizedPnl:bits(p.unrealizedPnl),realizedPnl:bits(p.realizedPnl),contracts:bits(p.contracts),liquidationPrice:bits(p.liquidationPrice),marginSize:bits(p.marginSize),lastUpdate:bits(p.lastUpdate),extra:{contractsNative:bits(p.extra.contractsNative),unrealizedPnlNative:bits(p.extra.unrealizedPnlNative)}}));
const markets=[
 {id:'BTC_USDT',symbol:'BTCUSDT',base:'BTC',quote:'USDT',contractSize:0.001,minLeverage:3,limits:{leverage:{min:2}}},
 {id:'ETH_USDT',symbol:'ETHUSDT',base:'ETH',quote:'USDT',contractSize:0.01,minLeverage:4,limits:{leverage:{min:5}}},
];
const rows=[{symbol:'BTC_USDT',positionType:2,positionMode:1,holdVol:'2',holdAvgPrice:'100',unrealized:'3',leverage:'5',openType:3,updateTime:1700000000000},{symbol:'OTHER_USDT',positionType:2,holdVol:'9',holdAvgPrice:'20'}];
const run=async(symbols,publicOnly=false)=>{
 let calls=0;
 const responseRows=structuredClone(rows);
 const raw=structuredClone(rows);
 const instance={isPublicDataOnly:publicOnly,accountPositionMode:'hedged',symbolSettings:new Map(),xhr:{get:async url=>{calls++;assert.equal(url,'/api/v1/private/position/open_positions');return {data:{data:responseRows}};}},store:{markets,marketsBySymbol:Object.fromEntries(markets.map(m=>[m.symbol,m])),positions:[]},emitAndLogError(error){throw Error(`unexpected fetchPositions error: ${error}`);}};
 const positions=await module.exports.fetchPositions.call(instance,symbols);
 const open=positions.find(position=>position.id==='BTCUSDT:short');
 let alias=false;
 if(open){open.info.__fetchPositionsAlias=true;alias=responseRows[0].__fetchPositionsAlias===true;}
 return {positions:view(positions),calls,alias,markets,raw};
};
const output=[];
for(const symbols of [undefined,['BTCUSDT'],[]])output.push({captureArchitecture:process.arch,captureNodeMajor:Number(process.versions.node.split('.')[0]),symbols: symbols??null,...await run(symbols)});
output.push({captureArchitecture:process.arch,captureNodeMajor:Number(process.versions.node.split('.')[0]),symbols:null,publicOnly:true,...await run(undefined,true)});
if(process.env.MEXC_FETCH_POSITIONS_CONTROL==='empty-source')output.length=0;
process.stdout.write(JSON.stringify(output));
