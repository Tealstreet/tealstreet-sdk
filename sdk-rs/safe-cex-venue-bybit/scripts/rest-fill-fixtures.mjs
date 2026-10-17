import ts from 'typescript';
import vm from 'node:vm';
import {readFileSync} from 'node:fs';
import assert from 'node:assert/strict';
import orderBy from 'lodash/orderBy.js';
import omitBy from 'lodash/omitBy.js';
import memoize from 'lodash/memoize.js';
import camelCase from 'lodash/camelCase.js';
import snakeCase from 'lodash/snakeCase.js';
const root = new URL('../../../packages/safe-cex/src/', import.meta.url);
const parse = file => ts.createSourceFile(file, readFileSync(new URL(file, root), 'utf8'), ts.ScriptTarget.Latest, true);
const emit = code => ts.transpileModule(code, {compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText;
const declaration = (source, name) => {
  const matches = source.statements.filter(ts.isVariableStatement).flatMap(s=>[...s.declarationList.declarations]).filter(d=>d.name.getText(source)===name);
  assert.equal(matches.length, 1, `source declaration ${name}`);
  return matches[0].initializer.getText(source);
};
const good = {execId:'fill1', orderId:'order1',symbol:'BTCUSD',execType:'Trade',execQty:'2',execPrice:'100',execTime:'1700000000000',side:'Buy',execFee:'.1',feeCurrency:'USDT',isMaker:true};
const recovered = {orderId:'recovered',orderStatus:'Filled',symbol:'BTCUSD',cumExecQty:'2',avgPrice:'100',side:'Sell',updatedTime:'1700000000000',cumExecFee:'-.2',feeCurrency:'USDT'};
const page = (list, nextPageCursor) => ({retCode:0,retMsg:'OK',result:{list,nextPageCursor}});
const cases=[];
for (const name of ['bybit','bybitdemo','bybitinverse']) {
  const file = name==='bybitinverse' ? name : 'bybit';
  const source=parse(`exchanges/bybit/${file}.exchange.ts`);
  const klass=source.statements.find(n=>ts.isClassDeclaration(n)&&n.members.some(m=>m.name?.getText(source)==='fetchHistoricalFills'));
  const names=['EXECUTION_QUERY_MAX_RANGE_MS','DEFAULT_EXECUTIONS_LOOKBACK_MS','getExecutionQueryWindows','getCategoryForSymbol','mapOrderHistoryRowsToExecutions','fetchExecutionsFallbackFromOrderHistory','fetchHistoricalFills'];
  const fields=names.map(name=>{
    const found=klass.members.filter(m=>m.name?.getText(source)===name);
    assert.equal(found.length,1,name);
    return `${name}=${found[0].initializer.getText(source)};`;
  }).join('\n');
  async function add(opts, executionPages, historyPages=[], publicOnly=false) {
    const calls=[], logs=[];
    const context=vm.createContext({orderBy,omitBy,memoize,camelCase,snakeCase,
      virtualClock:{getCurrentTimeMs:()=>1700000000000},
      console:{warn:message=>logs.push(`warn: ${message}`)},
    });
    for (const [name,file] of [['OrderSide','types.ts'],['LogSeverity','../../logging/src/LogSeverity.ts']]) {
      const types=parse(file);
      const decl=types.statements.find(n=>ts.isEnumDeclaration(n)&&n.name.text===name);
      assert.ok(decl,name);
      vm.runInContext(emit(decl.getText(types).replace(/^export /,'')),context);
    }
    for (const [file,names] of [
      ['utils/get-key.ts',['s','c','v']],['utils/omitUndefined.ts',['omitUndefined']],
      ['exchanges/bybit/bybit.inverse.ts',['isBybitInverseSymbol']],
      ['exchanges/bybit/bybit.options.ts',['isBybitOptionSymbol']],
      ['exchanges/bybit/bybit.types.ts',['ENDPOINTS']],
    ]) {
      const fileSource=parse(file);
      for (const name of names) vm.runInContext(emit(`globalThis.${name}=${declaration(fileSource,name)}`),context);
    }
    vm.runInContext(emit(`globalThis.Witness=class {${fields}}`),context);
    const owner=new context.Witness();
    owner.name=name; owner.store={markets:[]}; owner.isPublicDataOnly=publicOnly;
    owner.log=(message,severity='info')=>logs.push(`${severity}: ${message}`);
    const indexes=new Map();
    owner.xhr={get:async(url,{params})=>{
      calls.push({url,params});
      const pages=url==='/v5/execution/list'?executionPages:historyPages;
      assert.ok(['/v5/execution/list','/v5/order/history'].includes(url),url);
      const index=indexes.get(url)||0; indexes.set(url,index+1);
      return {data:pages.length?pages[Math.min(index,pages.length-1)]:page([])};
    }};
    const result=await owner.fetchHistoricalFills(opts);
    assert.ok(logs.every(message=>!message.includes("is not defined")), logs.join("\n"));
    cases.push({name,opts,executionPages,historyPages,publicOnly,calls,logs,result});
  }
  await add({symbol:'BTCUSD'},[page([good])]);
  await add({symbol:'BTCUSD',limit:2},[page([good],'next'),page([{...good,execId:'fill2'}])]);
  await add({symbol:'BTCUSD'},[page([good,null])]);
  await add({symbol:'BTCUSD',from:1},[page([{...good,execType:'Funding'}])],[page([recovered])]);
  await add({symbol:'BTCUSD',from:1},[page([],'loop')],[page([recovered])]);
  await add({symbol:'BTCUSD'},[page([{...good,execPrice:'0',execValue:'200',side:' sell '}])]);
  await add({symbol:'BTCUSD'},[page([])]);
  await add({symbol:'BTCUSD',from:1},[page([])]);
  await add({symbol:'BTCUSD',from:0,to:604800000},[page([])]);
  await add({symbol:'BTCUSD',limit:0},[page([{...good,execFee:'junk'}])]);
  await add({symbol:'BTCUSD',limit:-1},[page([good])]);
  await add({symbol:''},[page([good])]);
  await add({symbol:'BTCUSD'},[page([good])],[],true);
  await add({symbol:'BTCUSD',from:1},[page([])],[page([recovered],'next'),page([null])]);
  await add({symbol:'BTCUSD',from:1,limit:2},[page([])],[page([recovered,{...recovered,orderId:'newer',updatedTime:'1700000001000'}])]);
}
assert.equal(cases.length,45,'real reader source-case floor');
process.stdout.write(JSON.stringify(cases));
