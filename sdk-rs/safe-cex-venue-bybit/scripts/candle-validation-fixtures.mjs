import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import vm from 'node:vm';
import ts from 'typescript';
const source = ts.createSourceFile('bybit.ts',readFileSync(new URL('../../../packages/safe-cex/src/exchanges/bybit/bybit.exchange.ts',import.meta.url),'utf8'),ts.ScriptTarget.Latest,true);
const owner = source.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text==='BybitExchange');
const names = ['supportsTradeDerivedCandles','validateCandleInterval'];
const members = names.map(name=>{
  const selected = owner.members.filter(n=>n.name?.getText(source)===name);
  assert.equal(selected.length,1,`CANDLE-VALIDATION actual member ${name}`);
  return selected[0].getText(source);
});
const subject = vm.runInNewContext(ts.transpileModule(`new class { ${members.join('\n')} }`,{}).outputText);
assert.equal(subject.supportsTradeDerivedCandles,true);
const reports=[];
subject.emitAndLogError=error=>{reports.push(error.message);throw Error('unexpected report');};
const inputs=['1s','5s','30s','1m','1h','1d','s','','S','unknown',null,false,0,[],{}, {endsWith:null}];
const cases=inputs.map(input=>{
  let error=null,result;
  try{result=typeof subject.validateCandleInterval(input);}catch(e){error=e.message;}
  return {input,result,error};
});
assert.equal(cases.length,16,'CANDLE-VALIDATION source total');
assert.equal(reports.length,0,'CANDLE-VALIDATION source reports');
const hostile={kind:'throwing-endsWith',accessor:true,invocations:0,reads:0,result:null};
const hostileInput={get endsWith(){hostile.reads++;throw Error('eager interval read');}};
assert.equal(typeof Object.getOwnPropertyDescriptor(hostileInput,'endsWith').get,'function');
const recordHostileReturn=result=>{hostile.invocations++;hostile.result=typeof result;};
recordHostileReturn(subject.validateCandleInterval(hostileInput));
process.stdout.write(JSON.stringify({capture:{architecture:process.arch,node:process.versions.node},reports,cases,hostile}));
