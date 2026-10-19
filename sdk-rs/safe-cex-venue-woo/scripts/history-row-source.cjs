const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const file = path.join(__dirname, 'historical-fills-oracle.cjs');
const source = fs.readFileSync(file, 'utf8');
const entry = 'main().catch(error=>{console.error(error);process.exitCode=1;});';
assert.equal(source.split(entry).length, 2, 'history source definition admission');
const context = {require, __dirname, process, Buffer, console};
vm.runInNewContext(source.replace(entry, '') + '\nglobalThis.fixture={worker,members,pack,ast,cls};', context, {filename:file});
const {worker, members, pack, ast, cls} = context.fixture;
const retained = process.argv[2] === '--retained';
const boundary = process.argv[2] === '--boundary';
const inputs = retained || boundary ? [] : JSON.parse(process.argv[2]);
(async () => {
  const cases = [];
  if (boundary) {
    const native = value => {
      if (typeof value === 'string' && !value.isWellFormed()) return {$units:Array.from({length:value.length},(_,i)=>value.charCodeAt(i))};
      if (Array.isArray(value)) return value.map(native);
      if (value && typeof value === 'object') return Object.fromEntries(Object.entries(value).map(([k,v])=>[k,native(v)]));
      return pack(value);
    };
    for (const method of ['fetchHistoricalFills', 'fetchHistoricalTradesInternal']) {
      for (const profile of ['base', 'order-number', 'order-false', 'limit-string', 'limit-null', 'limit-array', 'from-string', 'from-null', 'to-infinity', 'unused-options', 'bad-asset', 'utf16-text', 'unknown-side', 'unknown-position']) {
        const subject = worker(members), logs = [];
        const row = {id:'fill', orderId:'17', symbol:'PERP_BTC_USDT', side:'BUY', executedPrice:17.125, executedQuantity:3.75, executedTimestamp:1700000000123, fee:-0, feeAsset:'USDT'};
        const rows = [{...row,id:'first'}, {...row,id:'middle'}, {...row,id:'last'}];
        const opts = {symbol:'BTCUSDT'};
        if (profile === 'order-number') opts.orderId = 17;
        if (profile === 'order-false') opts.orderId = false;
        if (profile === 'limit-string') opts.limit = '2';
        if (profile === 'limit-null') opts.limit = null;
        if (profile === 'limit-array') opts.limit = [2];
        if (profile === 'from-string') opts.from = '1700000000';
        if (profile === 'from-null') opts.from = null;
        if (profile === 'to-infinity') opts.to = Infinity;
        if (profile === 'unused-options') opts.unused = String.fromCharCode(0xd800);
        if (profile === 'bad-asset') rows[1].feeAsset = {label:'invalid'};
        if (profile === 'utf16-text') {rows[1].id = String.fromCharCode(0xd800); rows[1].orderId = String.fromCharCode(0xdc00); rows[1].feeAsset = String.fromCharCode(0xd800);}
        if (profile === 'unknown-side') rows[1].side = String.fromCharCode(0xd800);
        if (profile === 'unknown-position') rows[1].positionSide = String.fromCharCode(0xdc00);
        subject.isPublicDataOnly = false;
        subject.fetchTransactionHistoryRows = async () => rows;
        subject.emitAndLogError = message => logs.push(message);
        const result = await subject[method](opts);
        assert.equal(logs.length, 0);
        assert.equal(result.length, profile === 'limit-string' || profile === 'limit-array' || profile === 'unknown-side' ? 2 : profile === 'limit-null' ? 1 : 3);
        cases.push({method, profile, raw:JSON.stringify({rows}).replaceAll('"fee":0', '"fee":-0'), opts:native(opts), rows:native(result)});
      }
    }
    assert.equal(cases.length, 28);
    process.stdout.write(JSON.stringify(cases));
    return;
  }
  if (retained) {
    for (const method of ['fetchHistoricalFills', 'fetchHistoricalTradesInternal']) {
      for (const legacy of [false, true]) for (const ownUndefined of [false, true]) for (const location of ['envelope', 'row']) for (const shape of ['nestedRows', 'rows', 'nestedArray', 'array']) {
        const parser = cls.members.find(n => n.name?.getText(ast) === 'parseTransactionRowsFromResponse').getText(ast);
        const subject = worker(members + '\n' + parser), logs = [];
        const row = {id:'fill', orderId:'order', symbol:'PERP_BTC_USDT', side:'BUY', executedPrice:17.125, executedQuantity:3.75, executedTimestamp:1700000000123, fee:-0, realizedPnl:0, isMaker:0};
        const invalid = {...row, executedPrice:String.fromCharCode(0xd800), id:'invalid'};
        if (location === 'row') row.ignored = String.fromCharCode(0xdc00);
        if (ownUndefined) { row.executedPrice = undefined; row.executed_price = 123; row.price = 17.125; }
        const rows = [invalid, row];
        const data = shape === 'nestedRows' ? {data:{rows}} : shape === 'rows' ? {rows} : shape === 'nestedArray' ? {data:rows} : rows;
        if (location === 'envelope' && !Array.isArray(data)) data.ignored = String.fromCharCode(0xd800);
        subject.isPublicDataOnly = false;
        subject.fetchTransactionHistoryRows = async () => subject.parseTransactionRowsFromResponse(data);
        subject.emitAndLogError = message => logs.push(message);
        const output = await subject[method]({symbol:'BTCUSDT'});
        assert.equal(output.length, 1);
        assert.equal(logs.length, 0);
        cases.push({method, legacy, ownUndefined, location, shape, raw:JSON.stringify(data).replaceAll('\"fee\":0', '\"fee\":-0'), rows:pack(output)});
      }
    }
    assert.equal(cases.length, 64);
    process.stdout.write(JSON.stringify(cases));
    return;
  }
  for (const method of ['fetchHistoricalFills', 'fetchHistoricalTradesInternal']) {
    for (const extra of inputs) {
      const subject = worker(members), logs = [], calls = [];
      const rows = [extra, {id:'fill', orderId:'order', symbol:'PERP_BTC_USDT', side:'BUY', executedPrice:17.125, executedQuantity:3.75, executedTimestamp:1700000000123, fee:-0, realizedPnl:0, isMaker:0}];
      subject.isPublicDataOnly = false;
      subject.fetchTransactionHistoryRows = async query => {calls.push(query); return rows;};
      subject.emitAndLogError = message => logs.push(message);
      const output = await subject[method]({symbol:'BTCUSDT'});
      assert.equal(calls.length, 1, 'nonempty history page uses one logical fetch');
      assert.equal(output.length, extra === null || typeof extra === 'object' ? 1 : 0);
      assert.equal(logs.length, output.length ? 0 : 1);
      cases.push({method, extra, rows:pack(output), logs});
    }
  }
  assert.equal(cases.length, inputs.length * 2);
  process.stdout.write(JSON.stringify(cases));
})().catch(error => {console.error(error); process.exitCode=1;});
