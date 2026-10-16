const assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const {root, ts, load, header} = require('./types-oracle.cjs');
const profile = process.argv[2], control = process.argv.includes('--control');
assert.ok(['refresh-turn', 'kline-unit', 'input-prices', 'income-range-unit'].includes(profile), 'WEEX audit selected profile');
async function observe() {
  const cases = [];
  if (profile === 'refresh-turn') {
    const file = path.join(root, 'packages/safe-cex/src/exchanges/weex/weex.ws-private.ts');
    const ast = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
    const cls = ast.statements.find(n => ts.isClassDeclaration(n) && n.name.text === 'WeexPrivateWebsocket');
    assert.ok(cls, 'WEEX actual private class');
    for (const kind of ['Orders', 'Positions', 'Balance']) {
      const member = cls.members.find(n => n.name?.getText(ast) === 'throttledFetch' + kind);
      assert.ok(member, 'WEEX actual refresh member ' + kind);
      let text = member.getText(ast);
      if (control) {
        const needle = 'await this.parent.fetch' + kind + '()';
        assert.equal(text.split(needle).length - 1, 1, 'WEEX unique refresh await control');
        text = text.replace(needle, 'this.parent.fetch' + kind + '()');
      }
      const compiled = ts.transpileModule('export class Slice {' + text + '}', {compilerOptions:{module:ts.ModuleKind.CommonJS, target:ts.ScriptTarget.ES2022}});
      for (const scenario of ['success', 'disposed', 'rejected', 'apply-error']) {
        let callback, disposed = false;
        const events = [];
        const scope = {exports:{}, clearTimeout(){}, setTimeout(cb, ms){assert.equal(ms,250); callback=cb; return 1;}};
        vm.runInNewContext(compiled.outputText, scope);
        const instance = new scope.exports.Slice();
        Object.defineProperty(instance, 'isDisposed', {get(){return disposed;}});
        instance.parent = {['fetch'+kind](){events.push('fetch'); return scenario === 'rejected' ? Promise.reject(new RangeError('ready rejection')) : Promise.resolve('ready');}};
        function apply(value) {events.push('apply'); if(scenario === 'apply-error') throw new RangeError('apply'); assert.equal(value,'ready');}
        instance.store = {update(value){apply(value[kind.toLowerCase()]);}, addOrUpdatePositionsFull:apply};
        instance['throttledFetch'+kind]();
        const pending = callback();
        assert.deepEqual(events, ['fetch'], 'WEEX SOURCE ready refresh defers continuation');
        if(scenario === 'disposed') disposed = true;
        await pending;
        assert.deepEqual(events, scenario === 'success' || scenario === 'apply-error' ? ['fetch','apply'] : ['fetch']);
        cases.push({id:kind.toLowerCase()+':'+scenario, events});
      }
    }
    assert.equal(cases.length, 12, 'WEEX SOURCE refresh observation floor');
  } else if (profile === 'kline-unit') {
    const fixture = JSON.parse(fs.readFileSync(path.join(__dirname,'response-cases.json'),'utf8')).find(c => c.name === 'kline_surrogate');
    assert.ok(fixture, 'WEEX frozen kline surrogate fixture');
    const value = load(path.join(root,'packages/json-parse/src/jsonParse.ts')).jsonParse(fixture.raw);
    assert.equal(value.length,11,'WEEX SOURCE kline tuple floor');
    if(control) value[0]='replacement';
    assert.equal(typeof value[0],'string','WEEX SOURCE kline returned type');
    assert.deepEqual(Array.from({length:value[0].length},(_,i)=>value[0].charCodeAt(i)), [0xd800], 'WEEX SOURCE kline returned unit');
    cases.push({id:'tuple-0',units:[value[0].charCodeAt(0)]});
  } else if (profile === 'income-range-unit') {
    const file = path.join(root,'packages/safe-cex/src/exchanges/weex/weex.exchange.ts');
    const ast = ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
    const cls = ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text==='WeexExchange');
    const member = cls.members.find(n=>n.name?.getText(ast)==='fetchIncomeHistoryRange');
    assert.ok(member,'WEEX actual income range wrapper');
    const scope = {exports:{}, ...load(path.join(root,'packages/safe-cex/src/exchanges/income-paging.ts')),
      WEEX_INCOME_MAX_WINDOW_MS:8640000000, WEEX_INCOME_PAGE_LIMIT:100,
      ...load(path.join(root,'packages/safe-cex/src/types.ts')), async sleep(ms){assert.equal(ms,100);}};
    const text = ts.transpileModule('export class Slice {'+member.getText(ast)+'}',{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText;
    vm.runInNewContext(text,scope);
    const instance = new scope.exports.Slice();
    let calls=0;
    instance.fetchIncomeHistory=async opts=>{calls++;assert.equal(opts.limit,100);if(calls>1)return [];return [{symbol:'B/Q',incomeType:'FUNDING_FEE',income:3,asset:'Q',timestamp:1,tranId:JSON.parse('"\\ud800"')}];};
    instance.log=()=>assert.fail('WEEX two-point range must not truncate');
    const records = await instance.fetchIncomeHistoryRange(1,2);
    assert.equal(calls,3,'WEEX SOURCE range page floor');
    assert.equal(records.length,1,'WEEX SOURCE range returned record floor');
    if(control) records[0].tranId='replacement';
    assert.deepEqual(Array.from({length:records[0].tranId.length},(_,i)=>records[0].tranId.charCodeAt(i)),[0xd800],'WEEX SOURCE range returned unit');
    cases.push({id:'returned-tran-id',units:[records[0].tranId.charCodeAt(0)],count:records.length});
  } else {
    const names=['toNumber','normalizePercentage','normalizeTimestampToMs','normalizeTimestampToSeconds','normalizePositionMode','normalizeMarginType','normalizePositionSide','normalizeOrderSide','normalizeOrderStatus','normalizeOrderType','normalizeTriggerType','normalizeCandle','normalizeOrderBookSide','normalizeOrderBook'];
    const file=path.join(root,'packages/safe-cex/src/exchanges/weex/weex.utils.ts');
    const ast=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
    const selected=ast.statements.filter(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>names.includes(d.name.getText(ast))));
    assert.equal(selected.length,14,'WEEX SOURCE input price declarations');
    const text=selected.map(n=>n.getText(ast)).join('\n')+'\nexport {normalizePercentage,normalizeTriggerType};';
    const scope={exports:{},...load(path.join(root,'packages/safe-cex/src/types.ts')),...load(path.join(root,'packages/safe-cex/src/exchanges/weex/weex.types.ts')),...load(path.join(root,'packages/safe-cex/src/utils/orderbook.ts'))};
    vm.runInNewContext(ts.transpileModule(text,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,scope);
    let deep='.25';for(let i=0;i<150;i++) deep=[deep];
    const unit=JSON.parse('"\\ud800"');
    const expected={toNumber:[0,.25],normalizePercentage:[20,25],normalizeTimestampToMs:[0,250],normalizeTimestampToSeconds:[0,0],normalizePositionMode:['oneway','oneway'],normalizeMarginType:['cross','cross'],normalizePositionSide:['long','long'],normalizeOrderSide:[undefined,undefined],normalizeOrderStatus:['open','open'],normalizeOrderType:['limit','limit'],normalizeTriggerType:['last_price','last_price']};
    for(const name of names) for(const [index,unread] of [unit,deep].entries()) {
      let input=unread;
      if(name==='normalizeCandle') input={timestamp:1700000000000,open:'11',high:'13',low:'7',close:'12',volume:'5',ignored:unread};
      if(name==='normalizeOrderBookSide') input=[['1','2',unread]];
      if(name==='normalizeOrderBook') input={bids:[['1','2']],asks:[['3','4']],ignored:unread};
      const value=scope.exports[name](input,...(name==='normalizePercentage'?[12,10]:[]));
      if(name==='normalizeCandle') assert.deepEqual({...value},{timestamp:1700000000,open:11,high:13,low:7,close:12,volume:5},'WEEX SOURCE input price candle');
      else if(name==='normalizeOrderBookSide') assert.deepEqual(Array.from(value,v=>({...v})),[{price:1,amount:2,total:0}],'WEEX SOURCE input price level');
      else if(name==='normalizeOrderBook') assert.deepEqual({bids:Array.from(value.bids,v=>({...v})),asks:Array.from(value.asks,v=>({...v}))},{bids:[{price:1,amount:2,total:2}],asks:[{price:3,amount:4,total:4}]},'WEEX SOURCE input price book');
      else assert.equal(value,expected[name][index],'WEEX SOURCE input price '+name);
      cases.push({id:name+':'+(index?'depth150':'unit'),result:typeof value==='number'?value:typeof value==='string'?value:value===undefined?'undefined':'record'});
    }
    if(control) cases.pop();
    assert.equal(cases.length,28,'WEEX SOURCE input price case floor');
  }
  process.stdout.write(JSON.stringify({...header, observations:[{id:profile,cases}]})+'\n');
}
observe().catch(error=>{console.error(error);process.exitCode=1;});
