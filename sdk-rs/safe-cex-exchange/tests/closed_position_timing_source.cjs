const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ts = require('typescript');
const filename = path.resolve(__dirname, '../../../packages/safe-cex/src/utils/resolveClosedPositions.ts');
const js = ts.transpileModule(fs.readFileSync(filename, 'utf8'), {
  compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
}).outputText;
const sandbox = { exports: {} };
vm.runInNewContext(js, sandbox, { filename });
(async () => {
  const profiles = {};
  for (const name of ['venue', 'derived', 'independent', 'held', 'rejected', 'unavailable']) {
    const trace = [];
    let release;
    const opts = { from: 11, to: 22, symbol: 'BTC', limit: 3 };
    const fetch = (kind) => (received) => {
      if (received !== opts) throw new Error('options identity');
      trace.push(kind);
      const done = () => {
        trace.push('done');
        if (name === 'rejected') throw new Error('fetch rejected');
        return [];
      };
      if (name === 'held') return new Promise(resolve => { release = () => resolve(done()); });
      try { return Promise.resolve(done()); } catch (error) { return Promise.reject(error); }
    };
    const source = {
      fetchClosedPnl: fetch('venue'),
      deriveClosedPositions: name === 'unavailable' ? undefined : fetch('derived'),
      closedPositionsShareLedgerSource: name === 'independent' ? false : undefined,
    };
    const pending = sandbox.exports.resolveClosedPositions(source, opts, ['derived', 'independent', 'unavailable'].includes(name));
    const before = [...trace];
    if (release) release();
    let result, error;
    try { result = await pending; } catch (caught) { error = caught.message; }
    profiles[name] = { before, after: trace, result, error };
  }
  process.stdout.write(JSON.stringify(profiles));
})().catch(error => { console.error(error); process.exitCode = 1; });
