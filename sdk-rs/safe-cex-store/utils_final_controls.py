#!/usr/bin/env python3
"""Serial, restored source mutations for the previously pending utility slice."""
import json, pathlib, subprocess, sys
ROOT = pathlib.Path(__file__).resolve().parents[2]
OUT = pathlib.Path(sys.argv[1]); OUT.mkdir(parents=True, exist_ok=True)
SHA = subprocess.check_output(['git','rev-parse','HEAD'], cwd=ROOT, text=True).strip()
controls = [
 ('secondary', 'store', 'remaining_utils.rs', '"grvt",', '"asterdex",', 'native_secondary_runtime_and_capture_flags'),
 ('development', 'store', 'remaining_utils.rs', 'node_env != Some("production")', 'node_env == Some("production")', 'native_secondary_runtime_and_capture_flags'),
 ('runtime', 'store', 'remaining_utils.rs', 'pub fn is_node_or_bun_runtime(process: Option<&Value>) -> bool {', 'pub fn is_node_or_bun_runtime(process: Option<&Value>) -> bool { if process.is_some() { return false; }', 'native_secondary_runtime_and_capture_flags'),
 ('capture', 'store', 'remaining_utils.rs', '"__tealstreetSigningCapture"', '"__brokenSigningCapture"', 'native_secondary_runtime_and_capture_flags'),
 ('merge', 'store', 'remaining_utils.rs', '!= Some(std::cmp::Ordering::Greater)', '== Some(std::cmp::Ordering::Less)', 'native_merge_identity_first_match_cutoff_nan_and_dedupe'),
 ('unknown', 'store', 'remaining_utils.rs', 'if not_sent.is_match(&message) {\n        return Ok(false);', 'if not_sent.is_match(&message) {\n        return Ok(true);', 'native_unknown_send_veto_response_precedence_and_codes'),
 ('blob', 'store', 'read_blob.rs', '.map(char::from)', '.map(|byte| char::from(byte.wrapping_add(1)))', 'native_blob_char_codes_members_padding_and_failures'),
 ('clock_interval', 'store', 'virtual_clock.rs', 'TIME_SYNC_INTERVAL_MS: u64 = 300_000', 'TIME_SYNC_INTERVAL_MS: u64 = 300_001', 'native_secondary_runtime_and_capture_flags'),
 ('clock_singleton', 'store', 'virtual_clock.rs', '        &VIRTUAL_CLOCK\n', '        Box::leak(Box::new(VirtualClock::default()))\n', 'native_secondary_runtime_and_capture_flags'),
 ('loop', 'exchange', 'loop_util.rs', '            5000.0', '            1.0', 'native_loop_warning_dedupe_sleep_then_visibility_and_defer'),
 ('closed', 'exchange', 'closed_positions.rs', '!source.closed_positions_share_ledger_source.unwrap_or(true)', 'source.closed_positions_share_ledger_source.unwrap_or(true)', 'native_closed_positions_provenance_independence_and_one_fetch'),
 ('wallet', 'crypto', 'wallet_utils.rs', '    private_key\n        .parse()', '    "0000000000000000000000000000000000000000000000000000000000000002"\n        .parse()', 'native_wallet_key_address_and_invalid_scalar_refusal'),
 ('uuid', 'crypto', 'uuid.rs', '        .iter()\n', '        .iter().rev()\n', 'native_uuid_forward_mask_removal_and_entropy_failure'),
 ('polling', 'transport', 'polling.rs', 'ALL_EXCHANGES: &str = "*"', 'ALL_EXCHANGES: &str = "all"', 'native_polling_wildcard_constant_and_fallback'),
]
results=[]
for name, crate, file, before, after, assertion in controls:
    path=ROOT/'sdk-rs'/('safe-cex-'+crate)/'src'/file
    original=path.read_bytes(); source=original.decode()
    assert source.count(before)==1, (name, source.count(before))
    args=['cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-'+crate,'--test','remaining_utils','--jobs','2',assertion,'--','--exact']
    try:
        path.write_text(source.replace(before,after,1))
        broken=subprocess.run(args,cwd=ROOT,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
        (OUT/(name+'-broken.log')).write_text(broken.stdout)
    finally:
        path.write_bytes(original)
    restored=subprocess.run(args,cwd=ROOT,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
    (OUT/(name+'-restored.log')).write_text(restored.stdout)
    caught=broken.returncode==101 and ('test '+assertion+' ... FAILED') in broken.stdout and "panicked at" in broken.stdout
    passed=restored.returncode==0 and '1 passed; 0 failed' in restored.stdout
    row=dict(name=name,crate=crate,call_site=str(path.relative_to(ROOT)),assertion=assertion,broken_exit=broken.returncode,restored_exit=restored.returncode,caught=caught,restored=passed)
    results.append(row)
    (OUT/'results.json').write_text(json.dumps(dict(sha=SHA,stopped_after=len(results),expected_controls=14,results=results),indent=2)+'\n')
    print(json.dumps(row),flush=True)
    if not caught or not passed:
        sys.exit('FINDING or harness failure: '+name+'; sources restored; see logs')
