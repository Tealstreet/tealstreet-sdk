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
const owners = ['parse_field', ...['futures', 'pm'].flatMap(family =>
  ['book','mark','ticker'].map(channel => `handle_${family}_${channel}_events`))];
const prefix = 'safe_cex_venue_binance::public_ticker_events::';
if (!args.length) {
  const live = spawnSync(process.execPath, ['--import', 'tsx',
    path.join(workspace, 'safe-cex-venue-binance/scripts/public-ticker-source.cjs')],
  {cwd: root, encoding:'utf8',maxBuffer:16*1024*1024});
  assert.equal(live.status,0,`${live.error??''}\n${live.stderr}`);
  const expected = JSON.parse(live.stdout);
  const contracts = loadContracts();
  assert.deepEqual(Object.keys(expected).sort(), [...owners].sort());
  for (const name of owners) assert.deepEqual(contracts[prefix+name].cases, expected[name],
    `${name}: live maintained TypeScript ticker behavior changed`);
}
const target = path.join(workspace,'target/accepted-inventory');
const measured = spawnSync(process.execPath,
  [path.join(workspace,'safe-cex-parity/scripts/accepted-set.mjs'),
    '--package=safe-cex-venue-binance','--jobs=1','--measure','--module=public_ticker_events'],
  {cwd:workspace,encoding:'utf8',maxBuffer:64*1024*1024,
    env:{...process.env,CARGO_BUILD_JOBS:'1',CARGO_TARGET_DIR:target}});
assert.equal(measured.status,0,`${measured.error??''}\n${measured.stdout}\n${measured.stderr}`);
process.stdout.write(measured.stdout);
const results = JSON.parse(fs.readFileSync(path.join(target,'accepted-set-results.json'),'utf8'));
assert.deepEqual(results.map(row=>row.name).sort(),
  [...owners,'Failure','TickerFields'].map(name=>prefix+name).sort());
for (const row of results) {
  if(args.length && owners.includes(row.name.slice(prefix.length))) {
    assert.ok(row.failures.some(message=>message.includes('unclassified deserializable type')),row.name);
  } else assert.deepEqual(row.failures,[],row.name);
}
console.log(args.length?'Seven ticker data owners fail by name; both reviewed aliases pass.':
  'Seven real native ticker data owners and both reviewed aliases PASS; live source cases match.');
