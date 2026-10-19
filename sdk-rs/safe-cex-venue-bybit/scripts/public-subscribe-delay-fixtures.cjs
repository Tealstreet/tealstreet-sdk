const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const chunk = require('lodash/chunk');
const flatten = require('lodash/flatten');

async function main() {
  const profiles = [];
  for (const [file, owner] of [['bybitspot.ws-public.ts', 'BybitPublicWebsocket'], ['bybitinverse.ws-public.ts', 'BybitInversePublicWebsocket']]) {
    const source = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/bybit', file);
    const ast = ts.createSourceFile(source, fs.readFileSync(source, 'utf8'), ts.ScriptTarget.Latest, true);
    const cls = ast.statements.find(node => ts.isClassDeclaration(node) && node.name.text === owner);
    const selected = cls.members.filter(node => node.name?.getText(ast) === 'subscribe');
    assert.equal(selected.length, 1, owner + ' maintained subscribe');
    let now = 0;
    const waits = [], sends = [], delays = [];
    const context = vm.createContext({ chunk, flatten, sleep(ms) {
      delays.push(ms);
      return new Promise(resolve => waits.push({at: now + ms, resolve}));
    }});
    vm.runInContext(ts.transpileModule(`class Subject { ${selected[0].getText(ast)} } globalThis.api = new Subject();`, {compilerOptions: {target: ts.ScriptTarget.ES2022}}).outputText, context);
    const api = context.api;
    api.topics = {tickers: Array.from({length:25}, (_, i) => `tickers.SUB${i}USD`)};
    api.send = frame => sends.push({at: now, args: JSON.parse(frame).args});
    let settled = false;
    const pending = api.subscribe().then(() => {settled = true;});
    assert.equal(sends.length, 1, 'first chunk precedes caller observation');
    for (const [at, count] of [[0,1], [9,1], [10,2], [19,2], [20,3], [29,3], [30,3]]) {
      now = at;
      for (const wait of waits.splice(0)) {
        if (wait.at <= now) wait.resolve(); else waits.push(wait);
      }
      await Promise.resolve(); await Promise.resolve();
      assert.equal(sends.length, count, owner + ' ten-ms subscribe boundary at ' + at);
      if (at < 30) assert.equal(settled, false, 'source awaits the last chunk too');
    }
    await pending;
    assert.equal(settled, true);
    assert.deepEqual(delays, [10,10,10]);
    assert.deepEqual(sends, chunk(api.topics.tickers,10).map((args,i) => ({at:i*10,args})));
    const before = sends.length;
    api.topics = {};
    await api.subscribe();
    assert.equal(sends.length, before, 'empty topics neither send nor sleep');
    assert.deepEqual(delays, [10,10,10]);
    profiles.push({owner,sends,delays});
  }
  console.log(JSON.stringify(profiles));
}
main().catch(error => { console.error(error); process.exitCode = 1; });
