#!/usr/bin/env python3
"""Run isolated compiling controls against the actual encoded-interceptor gate."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[3]
CRATE = ROOT / 'sdk-rs/safe-cex-venue-hyperliquid'


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    env = dict(os.environ, RUSTFLAGS='-Dwarnings')
    cargo = os.environ.get('CARGO') or shutil.which('cargo')
    assert cargo, 'cargo is required'
    sha = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
    command = [cargo, 'test', '--manifest-path', 'sdk-rs/Cargo.toml', '-p',
               'safe-cex-venue-hyperliquid', '--test', 'rate_encoded',
               'encoded_pacing_telemetry_and_optional_weight_native_values_precede_source',
               '--locked', '--jobs', '2']
    cases = [
        ('encoded-pacing', 'src/api.rs',
         'get_hyperliquid_rest_request_weight(&request.url, &data)',
         'get_hyperliquid_rest_request_weight(&request.url, &{if let Some(RestBody::Encoded(bytes))=&request.body{serde_json::from_slice::<Value>(bytes).unwrap_or(Value::Null)}else{data.clone()}})',
         'ENCODED native reserved weight at 1'),
        ('encoded-telemetry', 'src/api.rs',
         'let data = request_data(request).unwrap_or(Value::Null);',
         'let data = if let Some(RestBody::Encoded(bytes))=&request.body{serde_json::from_slice::<Value>(bytes).unwrap_or(Value::Null)}else{request_data(request).unwrap_or(Value::Null)};',
         'TELEMETRY native original data variant at 2'),
        ('omitted-default', 'src/rate_limit.rs',
         'let weight = weight.into().unwrap_or(1.0);',
         'let weight = weight.into().unwrap_or(2.0);',
         'DEFAULT native reservation at 0'),
    ]
    receipt = {'base_sha': sha, 'scope': 'three isolated native controls, indices0..2 through omitted-default; no exhaustive mutation claim', 'controls': []}
    for index, (name, file, old, new, assertion) in enumerate(cases):
        path = CRATE / file
        original = path.read_bytes()
        text = original.decode()
        assert text.count(old) == 1, 'exact isolated mutation site '+name
        record = {'index': index, 'name': name, 'call_site': str(path.relative_to(ROOT)),
                  'base_blob_sha256': hashlib.sha256(original).hexdigest(),
                  'command': ['cargo']+command[1:], 'named_assertion': assertion}
        try:
            path.write_text(text.replace(old, new, 1))
            broken = subprocess.run(command, cwd=ROOT, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
            (args.output/(name+'-broken.log')).write_text(broken.stdout)
            record.update(broken_exit=broken.returncode,
                          compiled='could not compile' not in broken.stdout and 'error[E' not in broken.stdout,
                          assertion_seen=assertion in broken.stdout)
            record['disposition'] = 'SURVIVOR' if broken.returncode == 0 else 'caught' if record['compiled'] and record['assertion_seen'] else 'INVALID_CONTROL'
        finally:
            path.write_bytes(original)
        restored = subprocess.run(command, cwd=ROOT, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        (args.output/(name+'-restored.log')).write_text(restored.stdout)
        record.update(restored_exit=restored.returncode, restored_one_test='1 passed; 0 failed' in restored.stdout,
                      restored_bytes=path.read_bytes() == original)
        receipt['controls'].append(record)
        (args.output/'receipt.json').write_text(json.dumps(receipt, indent=2)+'\n')
        print(json.dumps(record), flush=True)
        if record['disposition'] != 'caught' or restored.returncode or not record['restored_one_test']:
            raise RuntimeError('retained control finding requires inspection')
    receipt['head_unchanged'] = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip() == sha
    (args.output/'receipt.json').write_text(json.dumps(receipt, indent=2)+'\n')


if __name__ == '__main__':
    main()
