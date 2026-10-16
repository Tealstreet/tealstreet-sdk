import assert from 'node:assert/strict';
import { mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { validateBridgeProof } from './bridge-proof.mjs';
import { assertQualifiedTarget, splitQualifiedTarget } from './bridge-targets.mjs';

// Independent synthetic admission inputs. Never write into repository evidence directories.
const CASES = `series-reentry|native_series_callback_factory_can_reenter|already borrowed
iterator-completed-close|native_destructuring_closes_only_active_binding_errors|NATIVE-ITERATOR-CLOSE-PHASE
position-channel|native_private_position_channel_is_plural|NATIVE-POSITION-CHANNEL-VALUE
read-dispatch-swap|assembled_rest_dispatch_uses_distinct_native_endpoints|NATIVE-REST-DISPATCH-ENDPOINT
exchange-noop|assembled_exchange_fields_reach_real_bodies|NATIVE-EXCHANGE-BINDING
public-noop|assembled_socket_fields_reach_real_bodies|NATIVE-SOCKET-BINDING
method-receiver|assembled_account_posts_eagerly_then_suspends_with_original_receiver|NATIVE-ACCOUNT-POST-RECEIVER
await-turn-model|assembled_account_posts_eagerly_then_suspends_with_original_receiver|NATIVE-ACCOUNT-AWAIT-SUSPENDS
enum-comparison|assembled_mapping_reads_live_native_imports|NATIVE-MAPPING-NONMATCH-DEFAULT
account-field-swap|assembled_get_account_retains_decoded_native_uuid|NATIVE-ACCOUNT-UUID-IDENTITY
parse-float-order|native_parse_float_captures_callee_before_order_field_getters|NATIVE-PARSE-FLOAT-CAPTURE
parse-float-price-order|native_parse_float_captures_callee_before_order_field_getters|NATIVE-PARSE-FLOAT-CAPTURE
order-side-import|assembled_order_retains_imported_side_value|NATIVE-ORDER-SIDE-IMPORT
position-side-fresh-read|assembled_positions_retain_each_live_enum_read|NATIVE-POSITION-ENUM-READ-COUNTS
position-margin-import|assembled_positions_retain_each_live_enum_read|NATIVE-POSITION-ENUM-READ-COUNTS
socket-order-enum|socket_order_retains_selected_enum_after_side_getter|NATIVE-SOCKET-ORDER-SIDE-IDENTITY
socket-position-enum|socket_position_retains_side_and_margin_at_source_order|NATIVE-SOCKET-POSITION-ID-SIDE
socket-margin-enum|socket_position_retains_side_and_margin_at_source_order|NATIVE-SOCKET-POSITION-MARGIN-IDENTITY
public-constructor-binding|constructed_socket_fields_call_concrete_bodies|NATIVE-ASSEMBLED-SOCKET-FIELD
private-constructor-binding|constructed_socket_fields_call_concrete_bodies|NATIVE-ASSEMBLED-PRIVATE-CHANNEL-COUNT
exchange-constructor-binding|constructed_exchange_get_account_calls_concrete_body_with_lexical_receiver|NATIVE-ASSEMBLED-ACCOUNT-PROMISE|runtime
proof-admission-guard|proof_admission_requires_independent_control_pairs|BRIDGE-PROOF-GUARD
source-noop|source_body_entry_floor_matches_native_after_native_assertions|SOURCE-BRIDGE-ORACLE`.split('\n').map(line => line.split('|'));

function fixture() {
  const sha = '1'.repeat(40);
  const receipts = [];
  const logs = new Map();
  assert.equal(CASES.length, 23);
  for (const [control, name, marker, target = 'lib'] of CASES) {
    const qualified = target === 'runtime' ? name : 'native_bindings::tests::' + name;
    for (const phase of ['mutated', 'restored']) {
      const mutation = phase === 'mutated';
      const log = `${receipts.length}-${control}-${phase}.log`;
      receipts.push({ sha, label: control + '-' + phase, runtime_executed: true,
        expected_exit: mutation ? 101 : 0, exit: mutation ? 101 : 0,
        outcome: mutation ? 'CAUGHT' : 'RESTORED-PASS', named_assertion: mutation ? marker : null, log,
        command: ['cargo', 'test', '--locked', '--manifest-path', 'sdk-rs/Cargo.toml', '-p',
          'safe-cex-venue-cryptocom', '--jobs', '1',
          ...(target === 'runtime' ? ['--test', 'runtime', qualified] : ['--lib', qualified]),
          '--', '--exact', '--include-ignored', '--nocapture', '--test-threads=1'] });
      logs.set(log, 'SYNTHETIC UNIT INPUT; NOT AN EXECUTION RECEIPT\nrunning 1 test\n' +
        (mutation ? `test ${qualified} ... FAILED\nthread '${qualified}' panicked at synthetic.rs:1:1:\n${marker}\ntest result: FAILED. 0 passed; 1 failed; 0 ignored;\n`
          : `test ${qualified} ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored;\n`));
    }
  }
  return { sha, receipts, logs };
}
function check(f) { return validateBridgeProof(f.receipts, { sha: f.sha, logs: f.logs }); }

test('admits independently enumerated synthetic 23-pair proof', () => {
  const f = fixture();
  const directory = mkdtempSync(path.join(tmpdir(), 'cryptocom-proof-synthetic-unit-'));
  try {
    writeFileSync(path.join(directory, 'SYNTHETIC-INPUT.json'), JSON.stringify(f.receipts));
    assert.deepEqual(check(f), { sha: f.sha, controls: 23, receipts: 46, stop: 'index22 source-noop' });
  } finally { rmSync(directory, { recursive: true }); }
});
test('rejects duplicated control replacing another expected pair', () => {
  const f = fixture(); f.receipts[44] = { ...f.receipts[0] };
  assert.throws(() => check(f), /duplicate proof label/);
});
test('rejects all-restored receipts with misleading count46', () => {
  const f = fixture();
  for (const r of f.receipts) Object.assign(r, { expected_exit: 0, exit: 0, outcome: 'RESTORED-PASS', named_assertion: null });
  assert.throws(() => check(f), /wrong expected exit/);
});
test('rejects a missing receipt', () => {
  const f = fixture(); f.receipts.pop(); assert.throws(() => check(f), /independent floor/);
});
test('rejects an unknown label replacing a required control', () => {
  const f = fixture(); f.receipts[0].label = 'invented-mutated'; assert.throws(() => check(f), /missing expected proof label/);
});
test('rejects mismatched metadata assertion marker', () => {
  const f = fixture(); f.receipts[0].named_assertion = 'WRONG'; assert.throws(() => check(f), /wrong assertion marker/);
});
test('rejects marker missing from raw runtime failure', () => {
  const f = fixture(); const name = f.receipts[0].log; f.logs.set(name, f.logs.get(name).replace('already borrowed', 'unrelated panic'));
  assert.throws(() => check(f), /missing raw assertion marker/);
});
test('rejects compiler-blocked log even when receipt claims CAUGHT', () => {
  const f = fixture(); const name = f.receipts[0].log;
  f.logs.set(name, f.logs.get(name) + 'error[E0277]: could not compile test\n');
  assert.throws(() => check(f), /compiler-blocked proof/);
});
test('rejects compiler-blocked outcome', () => {
  const f = fixture(); f.receipts[0].outcome = 'COMPILER-BLOCKED'; assert.throws(() => check(f), /wrong outcome/);
});
test('rejects wrong SHA, unexecuted runtime, wrong exit and missing raw log', () => {
  for (const [field, value, message] of [['sha', '2'.repeat(40), /wrong proof SHA/], ['runtime_executed', false, /runtime not executed/], ['exit', 0, /wrong measured exit/]]) {
    const f = fixture(); f.receipts[0][field] = value; assert.throws(() => check(f), message);
  }
  const f = fixture(); f.logs.delete(f.receipts[0].log); assert.throws(() => check(f), /missing raw proof log/);
});
test('rejects traversal and repeated log filenames', () => {
  const f = fixture(); f.receipts[0].log = '../outside.log'; assert.throws(() => check(f), /unsafe proof log/);
  const g = fixture(); g.receipts[1].log = g.receipts[0].log; assert.throws(() => check(g), /reused proof log/);
});
test('rejects wrong named command, wrong panic test and failed restoration', () => {
  const f = fixture(); f.receipts[0].command[0] = 'echo'; assert.throws(() => check(f), /wrong named proof command/);
  const g = fixture(); const name = g.receipts[0].log;
  g.logs.set(name, g.logs.get(name).replace("thread 'native_bindings::tests::native_series_callback_factory_can_reenter'", "thread 'unrelated'"));
  assert.throws(() => check(g), /wrong panic test/);
  const h = fixture(); h.logs.set(h.receipts[1].log, 'running 1 test\ntest result: FAILED. 0 passed; 1 failed;');
  assert.throws(() => check(h), /missing restored single pass/);
  const i = fixture(); const restored = i.receipts[1].log;
  i.logs.set(restored, i.logs.get(restored).replace('test native_bindings::tests::native_series_callback_factory_can_reenter ... ok', 'test unrelated ... ok'));
  assert.throws(() => check(i), /wrong restored test/);
});
test('mapped target parsing retains the complete qualified owner and method', () => {
  assert.deepEqual(splitQualifiedTarget('safe-cex-venue-cryptocom/src/native_bindings.rs::BoundHost::get'),
    ['safe-cex-venue-cryptocom/src/native_bindings.rs', 'BoundHost::get']);
  assert.throws(() => splitQualifiedTarget('BoundHost'), /invalid mapped target/);
});
test('qualified target check accepts actual alias and public inherent ownership', () => {
  assertQualifiedTarget('pub type Prepared<V> = (V,V);', 'Prepared');
  assertQualifiedTarget('pub struct Awaited<V,E> { value: V, error: E }', 'Awaited');
  assertQualifiedTarget('impl<V,E> BoundHost<V,E> { pub fn get(&self) {} }', 'BoundHost::get');
});
test('qualified target check rejects different owner and trait-impl-only leaf', () => {
  assert.throws(() => assertQualifiedTarget('impl Other { pub fn get(&self) {} }', 'BoundHost::get'), /owner mismatch/);
  assert.throws(() => assertQualifiedTarget('impl<V,E> ReadHost for BoundHost<V,E> { fn get(&self) {} }', 'BoundHost::get'), /owner mismatch/);
});
test('importing writer does not invoke CLI or mutate manifests', async () => {
  const writer = await import('./bridge-manifest.mjs');
  assert.equal(typeof writer.main, 'function');
  assert.equal(typeof writer.validateBridgeProof, 'function');
});
