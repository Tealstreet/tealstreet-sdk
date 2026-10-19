const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const chunk = require('lodash/chunk');
async function main() {
  const profiles = [];
  for (const [file, owner] of [['bybitspot.ws-public.ts', 'BybitPublicWebsocket'], ['bybitinverse.ws-public.ts', 'BybitInversePublicWebsocket']]) {
    const source = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/bybit', file);
    const ast = ts.createSourceFile(source, fs.readFileSync(source, 'utf8'), ts.ScriptTarget.Latest, true);
    const cls = ast.statements.find(node => ts.isClassDeclaration(node) && node.name.text === owner);
    const selected = cls.members.filter(node => node.name?.getText(ast) === 'onMarketsChanged');
    assert.equal(selected.length, 1, owner + ' maintained member');
    let now = 0;
    const waits = [], sends = [], delays = [];
    const context = vm.createContext({ chunk, sleep(ms) {
      delays.push(ms);
      return new Promise(resolve => waits.push({at: now + ms, resolve}));
    }});
    vm.runInContext(ts.transpileModule(`class Subject { ${selected[0].getText(ast)} } globalThis.api = new Subject();`, {compilerOptions: {target: ts.ScriptTarget.ES2022}}).outputText, context);
    const api = context.api;
    api.ws = {};
    api.isDisposed = false;
    api.topics = {tickers: []};
    api.getCurrentTickerTopics = () => api.topics.tickers;
    api.send = frame => sends.push({at: now, args: JSON.parse(frame).args});
    let settled = false;
    const pending = api.onMarketsChanged({added: Array.from({length: 23}, (_, i) => ({symbol: `DELAY${i}`}))}).then(() => {settled = true;});
    const expected = Array.from({length: 23}, (_, i) => `tickers.DELAY${i}`);
    assert.deepEqual(Array.from(api.topics.tickers), expected, 'topic state committed before first wait');
    async function advance(to) {
      now = to;
      for (const wait of waits.splice(0)) {
        if (wait.at <= now) wait.resolve(); else waits.push(wait);
      }
      await Promise.resolve(); await Promise.resolve();
    }
    for (const [at, count] of [[0,1], [4,1], [5,2], [9,2], [10,3], [14,3]]) {
      await advance(at);
      assert.equal(sends.length, count, owner + ' exact five-ms boundary at' + at);
      assert.equal(settled, false, 'source awaits the final chunk delay too');
    }
    await advance(15); await pending;
    assert.equal(settled, true);
    assert.deepEqual(delays, [5,5,5]);
    assert.deepEqual(sends, chunk(expected,10).map((args,i) => ({at:i*5,args})));
    profiles.push({owner, sends, delays});
  }
  console.log(JSON.stringify(profiles));
}
main().catch(error => { console.error(error); process.exitCode = 1; });
