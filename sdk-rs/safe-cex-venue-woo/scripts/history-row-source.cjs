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
const inputs = retained ? [] : JSON.parse(process.argv[2]);
(async () => {
  const cases = [];
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
