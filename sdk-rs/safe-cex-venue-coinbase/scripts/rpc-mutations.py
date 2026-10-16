#!/usr/bin/env python3
"""Compiling, sequential controls for the new RPC slice; restore bytes even on failure."""
from pathlib import Path
import hashlib
import json
import os
import subprocess
import sys

ROOT = Path.cwd()
CRATE = ROOT / 'sdk-rs/safe-cex-venue-coinbase'
SHA = subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip()
OUT = Path(sys.argv[1]) if len(sys.argv) > 1 else Path('/tmp/coinbase-rpc-controls') / SHA[:10]
OUT.mkdir(parents=True, exist_ok=True)
ENV = {**os.environ, 'SAFE_CEX_TYPESCRIPT_PATH': '/Users/sam/cs/tealstreet-next/node_modules/typescript'}
CARGO = ['cargo', 'test', '--manifest-path', 'sdk-rs/Cargo.toml', '-p', 'safe-cex-venue-coinbase', '--jobs', '2', '--test', 'rpc']
RESULTS = []

def run(label, test, expected, diagnostic=None):
    command = CARGO + [test, '--', '--exact']
    result = subprocess.run(command, env=ENV, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    (OUT / (label + '.log')).write_text(result.stdout)
    record = {'label': label, 'code_sha': SHA, 'command': command, 'exit': result.returncode,
              'expected_exit': expected, 'diagnostic': diagnostic,
              'diagnostic_found': diagnostic is None or diagnostic in result.stdout,
              'diff_sha256': hashlib.sha256(subprocess.check_output(['git', 'diff'])).hexdigest()}
    RESULTS.append(record)
    (OUT / 'results.json').write_text(json.dumps(RESULTS, indent=2) + '\n')
    print(json.dumps(record), flush=True)
    if result.returncode != expected or not record['diagnostic_found']:
        print(result.stdout[-10000:], flush=True)
        raise RuntimeError(label + ': unexpected gate result; a compiling survivor must be reported')

def control(label, old, new, test, diagnostic):
    path = CRATE / 'src/starbase_rpc.rs'
    original = path.read_bytes()
    source = original.decode()
    if source.count(old) != 1:
        raise RuntimeError(label + ': expected one mutation call site')
    try:
        path.write_text(source.replace(old, new, 1))
        run(label + '-mutated', test, 101, diagnostic)
    finally:
        path.write_bytes(original)
    run(label + '-restored', test, 0)

control('params-alias', '            params,\n',
        '            params: CoinbaseStarbaseJsonRpcParams::new(params.borrow().clone()),\n',
        'request_aliases_eager_ids_and_transport_failures_match_live_source',
        'request.borrow().params.shares_object(&input)')
control('request-id', 'self.next_request_id.set(id + 1.0);', 'self.next_request_id.set(id + 2.0);',
        'client_constructor_and_build_validation_match_live_source', 'native request ID')
control('rpc-error', 'code: error.code,', 'code: error.code + 1.0,',
        'rpc_error_and_declared_response_variants_keep_native_fields', 'native RPC error code')
control('response-value', 'Ok(object.remove("result").expect("own result"))', 'Ok(Value::Null)',
        'client_response_precedence_and_native_fields_match_live_source', 'decoded native RPC result')
control('http-envelope', 'let url = format!("{}/{method}", context.endpoint.trim_end_matches(\'/\'));',
        'let url = format!("{}/{method}", context.endpoint);',
        'http_transport_builds_native_envelope_and_preserves_sender_error', 'native HTTP URL')
control('heartbeat-request', 'json!({"interval": self.interval_seconds})', 'json!({"interval": self.interval_seconds + 1.0})',
        'heartbeat_constructor_and_operations_match_live_source', 'native request params')
control('request-record-boundary',
        'pub struct CoinbaseStarbaseJsonRpcRequest {\n    pub jsonrpc: CoinbaseStarbaseJsonRpcVersion,\n    #[serde(serialize_with = "serialize_id")]\n    pub id: f64,\n    pub method: String,\n    pub params: CoinbaseStarbaseJsonRpcParams,\n}',
        'pub struct CoinbaseStarbaseJsonRpcRequest<Params = CoinbaseStarbaseJsonRpcParams> {\n    pub jsonrpc: CoinbaseStarbaseJsonRpcVersion,\n    #[serde(serialize_with = "serialize_id")]\n    pub id: f64,\n    pub method: String,\n    pub params: Params,\n}',
        'request_declaration_preserves_record_boundary_and_required_fields',
        'request uses the shared params handle, never an unconstrained scalar generic')
print('Seven controls caught; all source files restored byte for byte.', flush=True)
