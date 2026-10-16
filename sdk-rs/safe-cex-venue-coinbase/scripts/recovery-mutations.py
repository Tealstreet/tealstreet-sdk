#!/usr/bin/env python3
"""Sequential, restoring controls for the recovered Coinbase row families.
Run from the repository root. No cargo processes are launched concurrently.
"""
from pathlib import Path
import hashlib
import json
import os
import subprocess
import sys

ROOT = Path.cwd()
CRATE = ROOT / 'sdk-rs/safe-cex-venue-coinbase'
SHA = subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip()
OUT = Path(sys.argv[1]) if len(sys.argv) > 1 else Path('/tmp/coinbase-recovery-controls') / SHA[:10]
OUT.mkdir(parents=True, exist_ok=True)
ENV = {**os.environ, 'SAFE_CEX_TYPESCRIPT_PATH': '/Users/sam/cs/tealstreet-next/node_modules/typescript'}
CARGO = ['cargo', 'test', '--manifest-path', 'sdk-rs/Cargo.toml', '-p', 'safe-cex-venue-coinbase', '--jobs', '2']
RESULTS = []

def run(label, arguments, expected_exit, diagnostic=None):
    command = CARGO + arguments
    result = subprocess.run(command, env=ENV, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    (OUT / (label + '.log')).write_text(result.stdout)
    record = {'label': label, 'code_sha': SHA, 'command': command, 'exit': result.returncode,
              'expected_exit': expected_exit, 'diagnostic': diagnostic,
              'diagnostic_found': diagnostic is None or diagnostic in result.stdout,
              'diff_sha256': hashlib.sha256(subprocess.check_output(['git', 'diff'])).hexdigest()}
    RESULTS.append(record)
    (OUT / 'results.json').write_text(json.dumps(RESULTS, indent=2) + '\n')
    print(json.dumps(record), flush=True)
    if result.returncode != expected_exit or not record['diagnostic_found']:
        print(result.stdout[-14000:], flush=True)
        raise RuntimeError(label)

def control(label, relative, replace, arguments, diagnostic):
    path = CRATE / relative
    original = path.read_text()
    mutated = replace(original)
    if mutated == original:
        raise RuntimeError('control patch did not apply: ' + label)
    try:
        path.write_text(mutated)
        run(label + '-mutated', arguments, 101, diagnostic)
    finally:
        path.write_text(original)
    run(label + '-restored', arguments, 0)

control('snapshot', 'src/private_order_snapshot.rs',
        lambda s: s.replace('if raw_order_count >= COINBASE_USER_ORDER_SNAPSHOT_BATCH_SIZE', 'if self.raw_order_count >= COINBASE_USER_ORDER_SNAPSHOT_BATCH_SIZE'),
        ['--lib', 'private_order_snapshot::tests::snapshot_uses_raw_batch_size_and_preserves_map_identity_order', '--', '--exact'],
        'snapshot batch count fixture')
control('ticker', 'src/rest_reads.rs',
        lambda s: s.replace('                volume,\n                quote_volume,', '                volume: quote_volume,\n                quote_volume: volume,'),
        ['--test', 'spot_cfm_tickers'], 'spot CFM decoded volume identity')
control('public-context', 'src/starbase_public.rs',
        lambda s: s.replace('            if !is_supported_public_channel(channel) || !seen.insert(channel)', '            seen.clear();\n            if !is_supported_public_channel(channel) || !seen.insert(channel)'),
        ['--test', 'validate_public_context'], 'native context success branch')
control('public-error', 'src/starbase_public.rs',
        lambda s: s.replace(' || code.abs() > 9_007_199_254_740_991.0', ''),
        ['--test', 'public_test_error'], 'public_test_error_matches_live_source_and_preserves_optional_data')
control('websocket-types', 'src/starbase_session.rs',
        lambda s: s.replace('"ready" => Ok(Self::Ready)', '"ready" => Ok(Self::Disconnected)'),
        ['--test', 'ws_disconnect_reason'], 'websocket_session_literal_unions_match_closed_typescript_sets')
# Keep optional schema metadata intact while breaking omission at the actual serializer.
control('rpc-record', 'src/starbase.rs',
        lambda s: s.replace('#[serde(skip_serializing_if = "Option::is_none")]\n    pub id: Option<f64>', '#[serde(skip_serializing_if = "Option::is_some")]\n    pub id: Option<f64>'),
        ['--test', 'rpc_error_response_type'], 'absent ID is omitted while explicit-null error data is retained')
control('endpoint', 'src/starbase.rs',
        lambda s: s.replace('"https://drb.coinbase.com/api/v2"', '"https://drb.coinbase.com/api/v9"'),
        ['--test', 'starbase', 'starbase_endpoint_table_and_resolver', '--', '--exact'],
        'resolved endpoint')
control('crypto', 'src/utils.rs',
        lambda s: s.replace('vec![tag, 0x82, ((len >> 8) & 0xff) as u8, (len & 0xff) as u8]', 'vec![tag, 0x82, (len & 0xff) as u8, ((len >> 8) & 0xff) as u8]'),
        ['--test', 'crypto_encoding'], 'asn1Wrap')
control('credentials', 'src/utils.rs',
        lambda s: s.replace('"ES256"', '"BROKEN_ES256"') if '"ES256"' in s else s.replace('algorithm: CoinbaseSignatureAlgorithm::Es256', 'algorithm: CoinbaseSignatureAlgorithm::EdDsa'),
        ['--test', 'credentials', 'credential_normalization_matches_live_typescript', '--', '--exact'],
        'credential_normalization_matches_live_typescript')
control('decimal', 'src/decimal.rs',
        lambda s: s.replace('whole_valid && fraction_valid', '!whole_valid && fraction_valid'),
        ['--test', 'decimal_guards'], 'coinbase_decimal_guards_match_live_typescript')
control('private-health', 'src/private_health.rs',
        lambda s: s.replace('store.update_socket_health(&update)?;', 'store.update_socket_health(&update).ok();'),
        ['--lib', 'private_health::tests::subscriber_failure_propagates_after_health_commit_and_before_callbacks', '--', '--exact'],
        'expected original Store notification failure')
control('decoder-classification', 'src/utils.rs',
        lambda s: s.replace('pub enum CoinbaseSignatureAlgorithm', '#[derive(serde::Deserialize)]\npub enum CoinbaseSignatureAlgorithm'),
        ['--test', 'decoder_boundaries'], 'utils::CoinbaseSignatureAlgorithm: derived/strict/serialize-only classification')
control('real-targets', 'RECOVERY_ROWS.json',
        lambda s: s.replace('::is_coinbase_unsigned_decimal', '::missing_recovered_native_symbol', 1),
        ['--test', 'recovery_targets'], 'recovered target has no native Rust item')
print('All listed controls caught; all files restored byte for byte.', flush=True)
