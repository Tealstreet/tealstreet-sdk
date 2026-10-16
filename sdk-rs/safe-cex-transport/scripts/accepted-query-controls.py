#!/usr/bin/env python3
"""Disconnect reviewed query/diagnostic operations and restore exact source bytes."""
from pathlib import Path
import json
import os
import subprocess

ROOT = Path(__file__).resolve().parents[3]
PACKAGE = ROOT / 'sdk-rs/safe-cex-transport'
RUNNER = PACKAGE / 'scripts/accepted-query-native.mjs'
INVENTORY = Path('/tmp/transport-query-control-inventory.json')
RESULTS = Path('/tmp/transport-query-control-results.json')
CASES = [
 ('how_context.rs','how_context::TradeTunnelTimeoutError','name: "TradeTunnelTimeoutError",','name: "WrongTimeoutError",'),
 ('provider.rs','provider::axios_form_encode','Ok(output.finish())','Ok(format!("wrong{}", output.finish()))'),
 ('query.rs','query::deserialize_query_wire','.map(Some)','.map(|_| Some(QueryValue::Null))'),
 ('query.rs','query::query_json_projection','QueryValue::Undefined => return None,','QueryValue::Undefined => Value::Null,'),
 ('signature_rejection.rs','signature_rejection::response_data','response.body.json_value().unwrap_or(Value::Null)','{ let _ = response; Value::Null }'),
 ('signature_rejection.rs','signature_rejection::is_binance_style_signature_rejection','== -1022.0','== -1021.0'),
 ('tealstreet.rs','tealstreet::is_tealstreet_web_socket_url','.ends_with(".tealstreet.io")','.ends_with(".tealstreet.invalid")'),
 ('transport_resolution.rs','transport_resolution::resolve_account_transport','account.tealstreet_proxy == Some(true) || metadata.requires_cors','account.tealstreet_proxy == Some(true) && metadata.requires_cors'),
]
SELECTOR = ('transport_resolution.rs','.map_or_else(|| "SG".into(), |key| key.to_owned())','.map_or_else(|| "JA".into(), |key| key.to_owned())')


def runtime(name, restored):
    RESULTS.unlink(missing_ok=True)
    result = subprocess.run(['node',str(RUNNER),'--inventory='+str(INVENTORY),'--out='+str(RESULTS)],cwd=ROOT,capture_output=True,text=True)
    output = result.stdout + result.stderr
    suffix = 'restored' if restored else 'disconnected'
    Path(f'/tmp/transport-query-{name.split("::")[-1]}-{suffix}.log').write_text(output)
    assert RESULTS.exists(), f'{name}: fresh native receipts required; compilation is not a behavioral control\n{output}'
    rows = json.loads(RESULTS.read_text())
    failed = [row for row in rows if row['failures']]
    assert len(rows) == 9, (name,len(rows))
    if restored:
        assert result.returncode == 0 and not failed, output
    else:
        assert result.returncode != 0 and len(failed) == 1, output
        assert failed[0]['name'] == 'safe_cex_transport::' + name, failed
        assert all('function output mismatch' in error for error in failed[0]['failures']), failed
    print(f'{name}: {suffix} native {"PASS" if restored else "named output FAIL"}',flush=True)


def selector(restored):
    name = 'native_preferences_preserve_undefined_final_operands_and_region_order'
    env = os.environ.copy()
    env['SAFE_CEX_TYPESCRIPT_PATH'] = str(ROOT / 'node_modules/typescript')
    result = subprocess.run(['cargo','test','--locked','--jobs','1','-p','safe-cex-transport','--test','tealstreet_transport',name,'--','--exact'],cwd=ROOT/'sdk-rs',env=env,capture_output=True,text=True)
    output = result.stdout + result.stderr
    suffix = 'restored' if restored else 'disconnected'
    Path(f'/tmp/transport-query-default_proxy_region-{suffix}.log').write_text(output)
    assert 'could not compile' not in output, output
    if restored:
        assert result.returncode == 0 and '1 passed; 0 failed' in output, output
    else:
        assert result.returncode == 101 and f'test {name} ... FAILED' in output, output
    print(f'default_proxy_region: {suffix} actual native/source {"PASS" if restored else "named failure"}',flush=True)


# Discover the actual unmodified compiler owners. Mutations preserve every API
# signature and source line count; fresh native compilation observes the effect.
prepare = subprocess.run(['node',str(ROOT/'sdk-rs/safe-cex-parity/scripts/accepted-set.mjs'),'--package=safe-cex-transport','--inventory','--jobs=1'],cwd=ROOT/'sdk-rs',capture_output=True,text=True)
Path('/tmp/transport-query-control-discovery.log').write_text(prepare.stdout+prepare.stderr)
assert prepare.returncode == 0, prepare.stdout+prepare.stderr
metadata = subprocess.run(['cargo','metadata','--locked','--format-version=1'],cwd=ROOT/'sdk-rs',capture_output=True,text=True)
assert metadata.returncode == 0, metadata.stderr
INVENTORY.write_bytes((Path(json.loads(metadata.stdout)['target_directory'])/'accepted-set-inventory.json').read_bytes())
snapshots = {PACKAGE/'src'/filename:(PACKAGE/'src'/filename).read_bytes() for filename,*_ in CASES}
try:
    for filename,name,old,new in CASES:
        path = PACKAGE/'src'/filename
        source = snapshots[path].decode()
        assert source.count(old) == 1, (name,source.count(old))
        try:
            path.write_text(source.replace(old,new))
            runtime(name,False)
        finally:
            path.write_bytes(snapshots[path])
        runtime(name,True)
    filename,old,new = SELECTOR
    path = PACKAGE/'src'/filename
    source = snapshots[path].decode()
    assert source.count(old) == 1
    try:
        path.write_text(source.replace(old,new))
        selector(False)
    finally:
        path.write_bytes(snapshots[path])
    selector(True)
finally:
    for path,source in snapshots.items():
        path.write_bytes(source)
assert all(path.read_bytes() == source for path,source in snapshots.items())
print('Eight compiled runtime output faults and one native/source selector fault failed; exact restoration passed',flush=True)
