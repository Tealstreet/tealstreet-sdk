"""Fourteen production faults plus a target-resolution meta-control; retain every outcome."""
from pathlib import Path
import argparse
import hashlib
import json
import os
import re
import signal
import shutil
import subprocess

sdk = Path(__file__).resolve().parents[1]
repo = sdk.parent
parser = argparse.ArgumentParser()
parser.add_argument('--out', type=Path, required=True)
args = parser.parse_args()
args.out.mkdir(parents=True, exist_ok=True)
cargo = os.environ.get('CARGO') or shutil.which('cargo')
assert cargo
sha = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=repo, text=True).strip()
def guard_inputs():
    assert subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=repo, text=True).strip() == sha, 'HEAD changed during gate'
    subprocess.run(['git', 'diff', '--quiet', 'HEAD'], cwd=repo, check=True)
    untracked = subprocess.check_output(['git', 'ls-files', '--others', '--exclude-standard'], cwd=repo, text=True).splitlines()
    metadata = {'.aimux-brief.md', '.aimux-next.md', '.aimux-priority.md'}
    unexpected = [name for name in untracked if name not in metadata and not (repo / name).resolve().is_relative_to(args.out.resolve())]
    assert not unexpected, 'uncommitted source/fixture inputs: ' + repr(unexpected)
guard_inputs()

faults = []
def fault(name, path, old, new, crate, target, test, marker, ignored=False, library=False, required=()):
    faults.append(dict(name=name, path=path, old=old, new=new, crate=crate, target=target, test=test, marker=marker, ignored=ignored, library=library, required_fragments=list(required)))

fault('cache-value-rebuffer', 'safe-cex-candle-cache/src/types.rs',
'''        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?
            .decode_with(|decoder| {
                ChunkRecordWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)''',
'''        let object = serde_json::Map::<String, serde_json::Value>::deserialize(deserializer)?;
        ChunkRecordWire::deserialize(serde_json::Value::Object(object)).map_err(serde::de::Error::custom)''',
'safe-cex-candle-cache', 'numeric_ingress', 'chunk_record_numeric_ingress_preserves_native_bits', 'CACHE-NATIVE-DECODE', required=('lastAccessed/0/shared-str', 'numeric ingress forbids serde_json::Value buffers'))
fault('cache-zero-sign', 'safe-cex-candle-cache/src/types.rs',
'''            .decode_with(|decoder| {
                ChunkRecordWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })''',
'''            .decode_with(|decoder| {
                ChunkRecordWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map(|mut record| { if record.last_accessed == 0.0 { record.last_accessed = 0.0; } record })''',
'safe-cex-candle-cache', 'numeric_ingress', 'chunk_record_numeric_ingress_preserves_native_bits', 'CACHE-NATIVE-BITS', required=('lastAccessed/-0/bare-str',))
fault('proxy-zero-sign', 'safe-cex-exchange/src/proxy_token.rs',
'        .map_err(serde::de::Error::custom)\n    }\n}\nimpl safe_cex_num::numeric_ingress::JsNumberOutput',
'''        .map(|mut message| { if let BroadcastMessage::ProxyTokenUpdate { expires_at, .. } = &mut message { if *expires_at == 0.0 { *expires_at = 0.0; } } message })
        .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput''',
'safe-cex-exchange', 'numeric_ingress_findings', 'proxy_broadcast_numeric_ingress_preserves_native_bits', 'PROXY-NATIVE-BITS')
fault('okx-category-fallback', 'safe-cex-venue-okx/src/income.rs',
'TableHit::Inherited(member) => TableHit::Inherited(member),', 'TableHit::Inherited(_) => TableHit::Mapped(IncomeCategory::Other),',
'safe-cex-venue-okx', 'findings', 'inherited_income_refuses_only_when_category_reaches_output', 'INCOME-NATIVE-CATEGORY')
fault('okx-live-fallback', 'safe-cex-venue-okx/src/rest_operations.rs',
'TableHit::Inherited(member) => TableHit::Inherited(member),', 'TableHit::Inherited(_) => TableHit::Mapped("live"),',
'safe-cex-venue-okx', 'findings', 'inherited_lookup_preserves_native_state_and_deferred_refusal', 'LOOKUP-NATIVE-STATE')
fault('okx-eager-refusal', 'safe-cex-venue-okx/src/rest_operations.rs',
'        let Some(mut order) = map_orders(&mapped_row, live_state)?.into_iter().next() else {',
'''        if matches!(live_state, TableHit::Inherited(_)) { return Ok(ClientOrderLookup::NotFound { definitive: false }); }
        let Some(mut order) = map_orders(&mapped_row, live_state)?.into_iter().next() else {''',
'safe-cex-venue-okx', 'findings', 'inherited_lookup_preserves_native_state_and_deferred_refusal', 'LOOKUP-DEFERRED-MAPPER')
fault('okx-mapped-status-loss', 'safe-cex-venue-okx/src/rest_operations.rs',
'            TableHit::Missing => {}', '            TableHit::Missing => order.status = OrderStatus::Open,',
'safe-cex-venue-okx', 'findings', 'lookup_ordinary_status_fallback_and_json_row_spread', 'LOOKUP-NATIVE-STATUS-FALLBACK')
fault('okx-row-spread-loss', 'safe-cex-venue-okx/src/rest_operations.rs',
'spread_lookup_row(row).map_err(|e| RestFailure {', 'spread_lookup_row(row).map(|_| row.clone()).map_err(|e| RestFailure {',
'safe-cex-venue-okx', 'findings', 'lookup_ordinary_status_fallback_and_json_row_spread', 'LOOKUP-NATIVE-ROW-SPREAD')
fault('instrument-member-collapse', 'safe-cex-store/src/instrument_class.rs',
'AssetClass::PrototypeProperty(member)', 'AssetClass::PrototypeProperty({ let _ = member; safe_cex_num::ProtoMember::Constructor })',
'safe-cex-store', 'instrument_native', 'inherited_instrument_members_keep_native_identity_tags', 'INSTRUMENT-NATIVE-MEMBER')
fault('risk-prototype-guard-loss', 'safe-cex-store/src/risk.rs',
"fn require_array_bucket(key: &str, message: &'static str) -> Result<(), RiskError> {",
"fn require_array_bucket(key: &str, message: &'static str) -> Result<(), RiskError> {\n    if !key.is_empty() { return Ok(()); }",
'safe-cex', 'risk_recovery', 'prototype_risk_error_is_catchable_and_store_clearing_is_observable', 'prototype key must produce recoverable risk error')
fault('bitget-income-category-substitution', 'safe-cex-venue-bitget/src/income.rs',
'TableHit::Inherited(member) => (None, member.js_string()),', 'TableHit::Inherited(member) => (Some(IncomeCategory::Other), member.js_string()),',
'safe-cex-venue-bitget', 'findings_policy', 'inherited_income_category_omission_keeps_the_open_transaction_id', 'BITGET-NATIVE-OPTIONAL-CATEGORY')
fault('bitget-status-substitution', 'safe-cex-venue-bitget/src/orders.rs',
'        status: typed(order.status, "status")?,', '        status: { let _ = &order.status; OrderStatus::Open },',
'safe-cex-venue-bitget', 'findings_policy', 'inherited_order_fields_and_native_info_input_have_separate_prices', 'BITGET-NATIVE-CLOSED-OUTPUT')
fault('bitget-page-public-guard', 'safe-cex-venue-bitget/src/history_reads.rs',
'    if public_data_only {\n        return Ok(Vec::new());\n    }\n    let default_limit = Value::from(100);', '    if !public_data_only {\n        return Ok(Vec::new());\n    }\n    let default_limit = Value::from(100);',
'safe-cex-venue-bitget', 'bill_page', 'live_bill_page', '_fetchBillPage request/call ordering', ignored=True)
fault('stats-own-record-invention', 'safe-cex-exchange/src/websocket.rs',
'        if stats.contains_key(topic) || inherited_handler_member(topic).is_none() {', '        if true {',
'safe-cex-exchange', None, 'websocket::tests::live_ws_typescript_oracle', 'inherited stats must not become own records', ignored=True, library=True)
fault('target-resolution-meta', 'safe-cex-store/src/instrument_class.rs',
'pub fn bybit_instrument_class(', 'pub fn orphan_control_missing_instrument_class(',
'safe-cex-parity', 'orphan_findings_targets', 'assigned_finding_targets_resolve_in_the_real_source_index', 'ASSIGNED-TARGET-RESOLVES')

planned = [
    'cache-value-rebuffer', 'cache-zero-sign', 'proxy-zero-sign', 'okx-category-fallback',
    'okx-live-fallback', 'okx-eager-refusal', 'okx-mapped-status-loss',
    'okx-row-spread-loss', 'instrument-member-collapse', 'risk-prototype-guard-loss',
    'bitget-income-category-substitution', 'bitget-status-substitution',
    'bitget-page-public-guard', 'stats-own-record-invention',
    'target-resolution-meta',
]
assert [spec['name'] for spec in faults] == planned
assert len(planned) == 15
results = []
def interrupted(signum, _frame):
    raise KeyboardInterrupt('control interrupted by signal ' + str(signum))
signal.signal(signal.SIGTERM, interrupted)
signal.signal(signal.SIGINT, interrupted)
def execute(spec, stage):
    cmd=[cargo,'test','--manifest-path','sdk-rs/Cargo.toml','-p',spec['crate'],'--locked','--jobs','2']
    cmd += ['--lib'] if spec['library'] else ['--test',spec['target']]
    cmd += [spec['test'],'--','--exact','--nocapture']
    if spec['ignored']: cmd += ['--ignored']
    run=subprocess.run(cmd,cwd=repo,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
    log=run.stdout.replace(str(repo),'<worktree>').replace(str(Path.home()),'<home>').replace('/opt/homebrew','<tool-prefix>')
    name=spec['name']+'-'+stage+'.log'
    (args.out/name).write_text(log)
    return dict(exit=run.returncode, command=[Path(cmd[0]).name]+cmd[1:], log=name, selected_test=spec['test'], named_test_executed=bool(re.search(r'^test ' + re.escape(spec['test']) + r' \.\.\. (ok|FAILED)$', log, re.M))),log

def persist():
    (args.out/'controls.json').write_text(json.dumps(dict(sha=sha,planned_controls=planned,stop_at='target-resolution-meta',controls=results),indent=2)+'\n')

for spec in faults:
    path=sdk/spec['path'];original=path.read_bytes();text=original.decode()
    print('START',spec['name'],flush=True)
    row=dict(name=spec['name'],path=spec['path'],marker=spec['marker'],required_fragments=spec['required_fragments'],original_sha256=hashlib.sha256(original).hexdigest())
    results.append(row)
    row['anchor_count'] = text.count(spec['old'])
    if row['anchor_count'] != 1:
        row['finding'] = 'mutation anchor is not unique';persist();raise SystemExit(1)
    row['baseline'],base=execute(spec,'baseline');persist()
    if row['baseline']['exit'] or not row['baseline']['named_test_executed'] or not re.search(r'test result: ok\. [1-9][0-9]* passed;',base):
        row['finding']='baseline did not execute a passing test';persist();raise SystemExit(1)
    try:
        mutated = text.replace(spec['old'],spec['new'])
        row['mutated_sha256'] = hashlib.sha256(mutated.encode()).hexdigest()
        row['stage'] = 'mutant-about-to-run';persist()
        path.write_text(mutated)
        row['mutant'],log=execute(spec,'mutant')
        row['caught']=row['mutant']['exit']==101 and row['mutant']['named_test_executed'] and 'test result: FAILED.' in log and spec['marker'] in log and all(fragment in log for fragment in spec['required_fragments']) and 'error[E' not in log
        if not row['caught']:row['finding']='survivor or compile/other failure; inspect mutant log'
    finally:
        path.write_bytes(original)
        row['stage'] = 'source-restored';persist()
    assert hashlib.sha256(path.read_bytes()).hexdigest()==row['original_sha256']
    row['restored'],restored=execute(spec,'restored')
    row['restored_pass']=row['restored']['exit']==0 and row['restored']['named_test_executed'] and bool(re.search(r'test result: ok\. [1-9][0-9]* passed;',restored))
    try:
        guard_inputs();row['inputs_restored'] = True
    except Exception as error:
        row['inputs_restored'] = False;row['finding'] = repr(error);row['restored_pass'] = False
    row['stage'] = 'finished';persist()
    if not row['restored_pass']:raise SystemExit(1)
    print('END',spec['name'],[row[k]['exit'] for k in ['baseline','mutant','restored']], 'caught='+str(row['caught']),flush=True)
if any(not r.get('caught') or not r.get('restored_pass') for r in results):raise SystemExit(1)
