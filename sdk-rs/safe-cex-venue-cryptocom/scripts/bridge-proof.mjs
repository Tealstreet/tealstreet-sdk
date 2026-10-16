import assert from 'node:assert/strict';

// Independent admission contract. Do not derive this inventory from receipts.
const EXPECTED = [
  ['series-reentry', 'native_series_callback_factory_can_reenter', 'already borrowed'],
  ['iterator-completed-close', 'native_destructuring_closes_only_active_binding_errors', 'NATIVE-ITERATOR-CLOSE-PHASE'],
  ['position-channel', 'native_private_position_channel_is_plural', 'NATIVE-POSITION-CHANNEL-VALUE'],
  ['read-dispatch-swap', 'assembled_rest_dispatch_uses_distinct_native_endpoints', 'NATIVE-REST-DISPATCH-ENDPOINT'],
  ['exchange-noop', 'assembled_exchange_fields_reach_real_bodies', 'NATIVE-EXCHANGE-BINDING'],
  ['public-noop', 'assembled_socket_fields_reach_real_bodies', 'NATIVE-SOCKET-BINDING'],
  ['method-receiver', 'assembled_account_posts_eagerly_then_suspends_with_original_receiver', 'NATIVE-ACCOUNT-POST-RECEIVER'],
  ['await-turn-model', 'assembled_account_posts_eagerly_then_suspends_with_original_receiver', 'NATIVE-ACCOUNT-AWAIT-SUSPENDS'],
  ['enum-comparison', 'assembled_mapping_reads_live_native_imports', 'NATIVE-MAPPING-NONMATCH-DEFAULT'],
  ['account-field-swap', 'assembled_get_account_retains_decoded_native_uuid', 'NATIVE-ACCOUNT-UUID-IDENTITY'],
  ['parse-float-order', 'native_parse_float_captures_callee_before_order_field_getters', 'NATIVE-PARSE-FLOAT-CAPTURE'],
  ['parse-float-price-order', 'native_parse_float_captures_callee_before_order_field_getters', 'NATIVE-PARSE-FLOAT-CAPTURE'],
  ['order-side-import', 'assembled_order_retains_imported_side_value', 'NATIVE-ORDER-SIDE-IMPORT'],
  ['position-side-fresh-read', 'assembled_positions_retain_each_live_enum_read', 'NATIVE-POSITION-ENUM-READ-COUNTS'],
  ['position-margin-import', 'assembled_positions_retain_each_live_enum_read', 'NATIVE-POSITION-ENUM-READ-COUNTS'],
  ['socket-order-enum', 'socket_order_retains_selected_enum_after_side_getter', 'NATIVE-SOCKET-ORDER-SIDE-IDENTITY'],
  ['socket-position-enum', 'socket_position_retains_side_and_margin_at_source_order', 'NATIVE-SOCKET-POSITION-ID-SIDE'],
  ['socket-margin-enum', 'socket_position_retains_side_and_margin_at_source_order', 'NATIVE-SOCKET-POSITION-MARGIN-IDENTITY'],
  ['public-constructor-binding', 'constructed_socket_fields_call_concrete_bodies', 'NATIVE-ASSEMBLED-SOCKET-FIELD'],
  ['private-constructor-binding', 'constructed_socket_fields_call_concrete_bodies', 'NATIVE-ASSEMBLED-PRIVATE-CHANNEL-COUNT'],
  ['exchange-constructor-binding', 'constructed_exchange_get_account_calls_concrete_body_with_lexical_receiver', 'NATIVE-ASSEMBLED-ACCOUNT-PROMISE', 'runtime'],
  ['proof-admission-guard', 'proof_admission_requires_independent_control_pairs', 'BRIDGE-PROOF-GUARD'],
  ['source-noop', 'source_body_entry_floor_matches_native_after_native_assertions', 'SOURCE-BRIDGE-ORACLE'],
];

export function validateProofLogName(name) {
  assert(typeof name === 'string' && /^[A-Za-z0-9][A-Za-z0-9._-]*\.log$/.test(name), 'unsafe proof log name');
  return name;
}

// Pure: supplied logs are immutable strings; this function performs no I/O.
export function validateBridgeProof(receipts, { sha, logs }) {
  assert.match(sha, /^[a-f0-9]{40}$/, 'proof SHA must be a full commit ID');
  assert(Array.isArray(receipts), 'proof receipts must be an array');
  assert.equal(receipts.length, 46, 'independent floor: 23 mutated/restored pairs');
  assert(logs instanceof Map, 'proof logs must be supplied as a Map');
  const byLabel = new Map();
  const seenLogs = new Set();
  for (const record of receipts) {
    assert(record && typeof record === 'object', 'invalid proof record');
    assert(!byLabel.has(record.label), 'duplicate proof label: ' + record.label);
    byLabel.set(record.label, record);
    const name = validateProofLogName(record.log);
    assert(!seenLogs.has(name), 'reused proof log: ' + name);
    seenLogs.add(name);
  }
  for (const [control, test, marker, target = 'lib'] of EXPECTED) {
    const qualifiedTest = target === 'runtime' ? test : 'native_bindings::tests::' + test;
    for (const phase of ['mutated', 'restored']) {
      const label = control + '-' + phase;
      const record = byLabel.get(label);
      assert(record, 'missing expected proof label: ' + label);
      assert.equal(record.sha, sha, 'wrong proof SHA: ' + label);
      assert.equal(record.runtime_executed, true, 'runtime not executed: ' + label);
      const mutation = phase === 'mutated';
      assert.equal(record.expected_exit, mutation ? 101 : 0, 'wrong expected exit: ' + label);
      assert.equal(record.exit, mutation ? 101 : 0, 'wrong measured exit: ' + label);
      assert.equal(record.outcome, mutation ? 'CAUGHT' : 'RESTORED-PASS', 'wrong outcome: ' + label);
      assert.equal(record.named_assertion, mutation ? marker : null, 'wrong assertion marker: ' + label);
      const command = ['cargo', 'test', '--locked', '--manifest-path', 'sdk-rs/Cargo.toml',
        '-p', 'safe-cex-venue-cryptocom', '--jobs', '1',
        ...(target === 'runtime' ? ['--test', 'runtime', qualifiedTest] : ['--lib', qualifiedTest]),
        '--', '--exact', '--include-ignored', '--nocapture', '--test-threads=1'];
      assert.deepEqual(record.command, command, 'wrong named proof command: ' + label);
      const output = logs.get(record.log);
      assert.equal(typeof output, 'string', 'missing raw proof log: ' + label);
      assert.match(output, /^running 1 test\s*$/m, 'raw log must run one test: ' + label);
      assert(!/could not compile|error\[E\d+\]/i.test(output), 'compiler-blocked proof: ' + label);
      if (mutation) {
        assert.match(output, /^test result: FAILED\. 0 passed; 1 failed;/m, 'missing single runtime failure: ' + label);
        assert(output.includes("thread '" + qualifiedTest + "'"), 'wrong panic test: ' + label);
        assert(output.includes('panicked at'), 'missing runtime panic: ' + label);
        assert(output.includes(marker), 'missing raw assertion marker: ' + label);
      } else {
        assert.match(output, /^test result: ok\. 1 passed; 0 failed;/m, 'missing restored single pass: ' + label);
        assert(output.includes('test ' + qualifiedTest + ' ... ok'), 'wrong restored test: ' + label);
        assert(!output.includes('panicked at'), 'restoration contains panic: ' + label);
      }
    }
  }
  // Count and unique labels above ensure no unexpected label can replace a pair.
  return Object.freeze({ sha, controls: 23, receipts: 46, stop: 'index22 source-noop' });
}
