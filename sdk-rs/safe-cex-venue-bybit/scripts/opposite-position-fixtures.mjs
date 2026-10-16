import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import vm from 'node:vm';
import ts from 'typescript';
const text=readFileSync(new URL('../../../packages/safe-cex/src/exchanges/bybit/bybit.exchange.ts',import.meta.url),'utf8');
const ast=ts.createSourceFile('bybit.ts',text,ts.ScriptTarget.Latest,true);
// The maintained inverse predicate executes from its module (type-only imports).
const inverseHelpers={};
vm.runInNewContext(ts.transpileModule(readFileSync(new URL('../../../packages/safe-cex/src/exchanges/bybit/bybit.inverse.ts',import.meta.url),'utf8'),{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{exports:inverseHelpers,require(){throw Error('no runtime imports expected')}});
const owner=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text==='BybitExchange');
const members=owner.members.filter(n=>n.name?.getText(ast)==='_hasFullSizedPositionOppositeSide');
assert.equal(members.length,1);
const subject=vm.runInNewContext(ts.transpileModule(`new class { ${members[0].getText(ast)} }`,{}).outputText,
 {OrderSide:{Buy:'buy'},PositionSide:{Short:'short',Long:'long'},isBybitInverseSymbol:inverseHelpers.isBybitInverseSymbol});
const lists=[[],[['BTC','short','2']],[['BTC','long','3']],[['BTC','short','0']],
 [['BTC','long','-2']],[['BTC','long','NaN']],[['BTC','short','Infinity']],
 [['BTC','short','-Infinity']],[['OTHER','short','4']],
 [['BTC','long','1'],['BTC','short','5']],[['BTC','short','1'],['BTC','short','2']]];
const amounts=['-3','-0','0','1','2','2.5','NaN','Infinity','-Infinity'];
const cases=[];
for(const positions of lists)for(const symbol of ['BTC','OTHER'])for(const side of ['buy','sell'])for(const amount of amounts){
 subject.store={positions:positions.map(([symbol,side,contracts])=>({symbol,side,contracts:Number(contracts)}))};
 cases.push({positions,symbol,side,amount,expected:subject._hasFullSizedPositionOppositeSide(symbol,side,Number(amount))});
}
assert.equal(cases.length,396,'OPPOSITE source total');
const boundaries=[{symbol:'BTC',side:'short',contracts:'0'},{symbol:'BTC',side:'unexpected',contracts:2}].map(position=>{
 subject.store={positions:[position]};
 return {position,expected:subject._hasFullSizedPositionOppositeSide('BTC','buy',0)};
});
subject.store={positions:[{symbol:'BTC',side:'long',contracts:2}]};
const orderSideBoundary={side:'unexpected',expected:subject._hasFullSizedPositionOppositeSide('BTC','unexpected',1)};
subject.store={positions:[{symbol:'\ud800',side:'short',contracts:2}]};
const surrogate={input:JSON.stringify({symbol:'\ud800'}),expected:subject._hasFullSizedPositionOppositeSide('\ud800','buy',1)};
// Inverse amounts are USD contracts, compared with the position's native size.
const inverseLists=[[['short',0.02,1000]],[['short',0.02,null]],[['long',0.02,500]],[['short',0,1000]],[['long',0.01,700],['short',0.02,1200]]];
const inverseCases=[];
for(const positions of inverseLists)for(const side of ['buy','sell'])for(const amount of [500,1000,1500]){
 subject.store={positions:positions.map(([pside,contracts,native])=>({symbol:'BTCUSD',side:pside,contracts,...(native===null?{}:{extra:{contractsNative:native}})}))};
 inverseCases.push({positions,side,amount,expected:subject._hasFullSizedPositionOppositeSide('BTCUSD',side,amount)});
}
assert.equal(inverseCases.length,30,'OPPOSITE inverse total');
process.stdout.write(JSON.stringify({inverseCases,boundaries,orderSideBoundary,surrogate,capture:{architecture:process.arch,node:process.versions.node},cases}));
