import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { loadContracts } from '../../safe-cex-parity/scripts/accepted-contract-files.mjs';
const workspace = fileURLToPath(new URL('../..', import.meta.url));
const args = process.argv.slice(2);
assert.ok(args.length <= 1 && args.every(arg => arg === '--expect-unclassified'));
const functions = ['payload','add_handler','remove_handler','topic_add','legacy_topic_remove',
  'depth','listen_futures_book','listen_pm_book','listen_spot_book'];
const aliases = ['Action','Book','Job','Parser','Snapshot','SyncState','Unsubscribe'];
const prefix = 'safe_cex_venue_binance::public_listeners::';
if (!args.length) {
  const contracts = loadContracts();
  for (const owner of functions) {
    assert.equal(contracts[prefix+owner].category, 'non-decoder', owner);
  }
}
const target = path.join(workspace,'target/accepted-inventory');
const measured = spawnSync(process.execPath,
  [path.join(workspace,'safe-cex-parity/scripts/accepted-set.mjs'),
    '--package=safe-cex-venue-binance','--jobs=1','--measure','--module=public_listeners'],
  {cwd:workspace,encoding:'utf8',maxBuffer:64*1024*1024,
    env:{...process.env,CARGO_BUILD_JOBS:'1',CARGO_TARGET_DIR:target}});
assert.equal(measured.status,0,`${measured.error??''}\n${measured.stdout}\n${measured.stderr}`);
process.stdout.write(measured.stdout);
const results = JSON.parse(fs.readFileSync(path.join(target,'accepted-set-results.json'),'utf8'));
assert.deepEqual(results.map(row=>row.name).sort(),
  [...functions,...aliases].map(name=>prefix+name).sort());
for (const row of results) {
  if(args.length && functions.includes(row.name.slice(prefix.length))) {
    assert.ok(row.failures.some(message=>message.includes('unclassified deserializable type')),row.name);
  } else assert.deepEqual(row.failures,[],row.name);
}
console.log(args.length ? 'Nine listener functions fail missing classification by name; seven existing aliases pass.' :
  'Nine individual typed listener role classifications and all seven existing aliases PASS; capabilities are not executed by these diagnostic classifications.');
