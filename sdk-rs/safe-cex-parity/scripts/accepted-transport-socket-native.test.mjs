import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {spawnSync} from 'node:child_process';
import test from 'node:test';
import {inventory} from './accepted-inventory.mjs';
import {loadPolicy} from './accepted-policy-files.mjs';
import {loadContracts} from './accepted-contract-files.mjs';
import {contractFor} from './accepted-contracts.mjs';
import {runProbes} from './accepted-probes.mjs';

const root = fileURLToPath(new URL('../..', import.meta.url));
const owners = [
  'safe_cex_transport::how::HOWWebSocket',
  'safe_cex_transport::relay::RelayWebSocket',
  'safe_cex_transport::registry::TransportRegistry',
  'safe_cex_transport::tealstreet::TealstreetTransport',
];
function cargo(args, extraEnv = {}) {
  const result = spawnSync('cargo', args, {cwd:root, encoding:'utf8', maxBuffer:64*1024*1024,
    env:{...process.env, CARGO_BUILD_JOBS:'1', ...extraEnv}});
  assert.equal(result.status, 0, 'transport native preparation must compile:\n' + result.stdout + result.stderr);
  return result.stdout;
}
function probe() {
  const metadata = JSON.parse(cargo(['metadata','--locked','--no-deps','--format-version=1']));
  const pkg = metadata.packages.find(row => row.name === 'safe-cex-transport');
  assert.ok(pkg && metadata.workspace_members.includes(pkg.id));
  const target = pkg.targets.find(row => row.kind.includes('lib'));
  cargo(['rustdoc','--locked','-p',pkg.name,'--lib','--jobs','1','--','-Z','unstable-options',
    '--output-format=json','--document-private-items','--document-hidden-items','-A','rustdoc::all'],
  {RUSTC_BOOTSTRAP:'1'});
  const doc = JSON.parse(fs.readFileSync(path.join(metadata.target_directory,'doc',target.name + '.json')));
  const found = inventory(doc, {onlyOwners:owners, defaultTraits:loadPolicy('default-traits'),
    receiverTraits:loadPolicy('receiver-traits'), readSource:file=>fs.readFileSync(path.join(root,file))});
  assert.deepEqual(found.types.map(row=>row.name).sort(), owners.slice().sort());
  const contracts = loadContracts();
  const failures = [];
  for (const owner of found.types) {
    try { contractFor(owner, contracts); } catch (error) { failures.push(error.message); }
  }
  assert.deepEqual(failures, [], 'all four actual compiler owners need complete reviewed contracts');
  const expectedMethods = {
    HOWWebSocket:['new','send','send_token','get_latency_for_domain','get_all_latencies'],
    RelayWebSocket:['new','with_connection_options','close','on_inner_close'],
    TransportRegistry:['with_public_cors_proxy','resolve_transport_routing'],
    TealstreetTransport:['new','ws_routing','ws_connection'],
  };
  for (const owner of found.types) {
    assert.deepEqual([...owner.methods, ...(owner.traitMethods ?? [])].map(row=>row.name),
      expectedMethods[owner.ident], owner.name + ': independent API floor');
  }
  const selected = {...found, package:pkg.name, packageId:pkg.id, target, features:[],
    publicAliases:[], defaultWitnesses:[], locals:[], delegatedLocals:[]};
  const rows = runProbes(root, metadata, [selected], contracts, {target:'native', features:[], jobs:1});
  assert.deepEqual(rows.map(row=>row.name).sort(), owners.slice().sort());
  assert.ok(rows.every(row=>!row.classificationOnly), 'all four owners must execute real native observations');
  for (const row of rows) assert.deepEqual(row.failures, [], row.name);
}
const controls = [
  ['transport HOW auth wire token', 'how.rs',
    'self.send(json!({"type":"rpc.auth", "token":token, "source":source}))',
    'self.send(json!({"type":"rpc.auth", "token":format!("{token}-mutated"), "source":source}))', owners[0]],
  ['transport relay close code', 'relay.rs',
    'self.finish(code, reason, was_clean);',
    'self.finish(code.wrapping_add(1), reason, was_clean);', owners[1]],
  ['transport registry instance precedence', 'registry.rs',
    'self.get_transport(instance).public_cors_proxy()',
    'self.get_transport(instance.filter(|_| false)).public_cors_proxy()', owners[2]],
  ['transport provider foreign URL refusal', 'tealstreet.rs',
    'if !relay && !is_tealstreet_web_socket_url(url) {',
    'if !relay && !is_tealstreet_web_socket_url(url) && url.is_empty() {', owners[3]],
];
function child() {
  const result = spawnSync(process.execPath, [fileURLToPath(import.meta.url),'--probe'],
    {encoding:'utf8', maxBuffer:64*1024*1024});
  assert.ok(!result.error, String(result.error));
  const output = (result.stdout ?? '') + '\n' + (result.stderr ?? '');
  assert.doesNotMatch(output, /error\[E\d+\]|could not compile|preparation must compile|compile failure is not a control/);
  return {...result, output};
}
if (process.argv[2] === '--probe') {
  probe();
} else {
  test('Transport socket and provider contracts execute all four owners and reject production disconnects',
    {timeout:3600000}, () => {
      const initial = child();
      assert.equal(initial.status, 0, initial.output);
      assert.equal(controls.length, 4);
      for (const [name, file, before, after, owner] of controls) {
        const source = path.join(root,'safe-cex-transport/src',file);
        const original = fs.readFileSync(source);
        const text = original.toString();
        assert.equal(text.split(before).length - 1, 1, name + ': one actual production mutation');
        try {
          fs.writeFileSync(source, text.replace(before,after));
          assert.notDeepEqual(fs.readFileSync(source), original, name + ': mutation applied');
          const changed = child();
          assert.equal(changed.status, 1, changed.output);
          assert.ok(changed.output.includes(owner), changed.output);
          assert.match(changed.output, /function output mismatch/);
          console.log(name + ': named assertion; process exit 1');
        } finally {
          fs.writeFileSync(source,original);
        }
        assert.deepEqual(fs.readFileSync(source), original, name + ': exact source restoration');
        const restored = child();
        assert.equal(restored.status, 0, restored.output);
      }
    });
}
