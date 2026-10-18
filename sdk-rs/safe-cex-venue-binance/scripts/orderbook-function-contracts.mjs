import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { loadContracts } from '../../safe-cex-parity/scripts/accepted-contract-files.mjs';
const workspace = fileURLToPath(new URL('../..', import.meta.url));
const root = path.dirname(workspace);
const args = process.argv.slice(2);
assert.ok(args.length <= 1 && args.every(arg => arg === '--expect-unclassified'));
const owners = ['load_binance_order_book_snapshot','get_buffered_binance_depth_updates',
  'apply_binance_depth_update','depth_index'];
const prefix = 'safe_cex_venue_binance::orderbook::';
if (!args.length) {
  const live = spawnSync(process.execPath, ['--import', 'tsx',
    path.join(workspace, 'safe-cex-venue-binance/scripts/orderbook-function-source.cjs')],
  {cwd: root, encoding:'utf8',maxBuffer:16*1024*1024});
  assert.equal(live.status,0,`${live.error??''}\n${live.stderr}`);
  const expected = JSON.parse(live.stdout);
  const contracts = loadContracts();
  assert.deepEqual(Object.keys(expected).sort(), [...owners].sort());
  for (const name of owners) assert.deepEqual(contracts[prefix+name].cases, expected[name],
    `${name}: live maintained TypeScript orderbook behavior changed`);
}
const target = path.join(workspace,'target/accepted-inventory');
const measured = spawnSync(process.execPath,
  [path.join(workspace,'safe-cex-parity/scripts/accepted-set.mjs'),
    '--package=safe-cex-venue-binance','--jobs=1','--measure','--module=orderbook'],
  {cwd:workspace,encoding:'utf8',maxBuffer:64*1024*1024,
    env:{...process.env,CARGO_BUILD_JOBS:'1',CARGO_TARGET_DIR:target}});
assert.equal(measured.status,0,`${measured.error??''}\n${measured.stdout}\n${measured.stderr}`);
process.stdout.write(measured.stdout);
const results = JSON.parse(fs.readFileSync(path.join(target,'accepted-set-results.json'),'utf8'));
assert.deepEqual(results.map(row=>row.name).sort(),
  [...owners,'BinanceDepthUpdate'].map(name=>prefix+name).sort());
for (const row of results) {
  if(args.length && owners.includes(row.name.slice(prefix.length))) {
    assert.ok(row.failures.some(message=>message.includes('unclassified deserializable type')),row.name);
  } else assert.deepEqual(row.failures,[],row.name);
}
console.log(args.length?'Four orderbook data owners fail by name; the reviewed depth alias passes.':
  'Four real native orderbook data owners and the reviewed depth alias PASS; live source cases match.');
