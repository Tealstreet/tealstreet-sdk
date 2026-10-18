#!/usr/bin/env python3
"""Compile independent actual-production faults and restore exact bytes."""
import hashlib
import os
from pathlib import Path
import subprocess

root = Path(__file__).resolve().parents[2]
venue = root / 'safe-cex-venue-coinbase'
crypto = venue / 'src/binding/crypto.rs'
orders = venue / 'src/binding/orders.rs'
starbase = venue / 'src/starbase_orders.rs'
original = {p: p.read_bytes() for p in (crypto, orders, starbase)}
env = dict(os.environ, CARGO_INCREMENTAL='0')
if 'SAFE_CEX_TYPESCRIPT_PATH' not in env:
    common = subprocess.check_output(['git', 'rev-parse', '--git-common-dir'], cwd=root, text=True).strip()
    env['SAFE_CEX_TYPESCRIPT_PATH'] = str((root / common).resolve().parent / 'node_modules/typescript')
logs = root / 'target/coinbase-divergence-controls'
logs.mkdir(parents=True, exist_ok=True)

def run(label, target, library=False):
    args = ['cargo', 'test', '--locked', '--jobs', '2', '-p', 'safe-cex-venue-coinbase']
    args += ['--lib'] if library else ['--test', 'exchange_binding']
    args += [target, '--', '--exact', '--nocapture']
    result = subprocess.run(args, cwd=root, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    (logs / (label + '.log')).write_text(result.stdout)
    return result

def replaced(path, old, new):
    text = original[path].decode()
    assert text.count(old) == 1, (path, old)
    return text.replace(old, new).encode()

pem = 'divergence_pem_failure_reaches_real_ws_and_rest_bindings'
metadata = 'binding::crypto::tests::native_pem_decode_keeps_the_original_failure_metadata'
live = 'divergence_starbase_intent_loss_is_outside_live_placement'
controls = [
    ('pem-diagnostic', crypto, replaced(crypto, 'let mut error = ExchangeError::new(message);', 'let mut error = ExchangeError::new({ let _ = message; original.to_string() });'), pem, False, 'source PEM error visible at actual WS hook'),
    ('pem-name', crypto, replaced(crypto, 'error.name = "InvalidCharacterError".into();', 'error.name = "Error".into();'), metadata, True, 'InvalidCharacterError'),
    ('pem-cause', crypto, replaced(crypto, 'error.origin = Some(ExchangeErrorOrigin::new(original));', 'error.origin = { let _ = ExchangeErrorOrigin::new(original); None };'), metadata, True, 'original codec failure remains the error origin'),
    ('legacy-placement', orders, replaced(orders, 'order_create::place_order(self.order_host(), options, &self.shared.extra.spawn)', '{ let _: PreparedOrderOpts = options.into(); Promise::resolved(Ok::<_, CoinbaseOrderFailure>(vec!["lost-real-placement".into()])) }'), live, False, 'audit-legacy'),
]
for name, path, mutant, target, library, label in controls:
    for p, data in original.items():
        assert p.read_bytes() == data, 'dirty production input'
    baseline = run(name + '-baseline', target, library)
    assert baseline.returncode == 0 and '1 passed' in baseline.stdout, baseline.stdout
    try:
        path.write_bytes(mutant)
        result = run(name + '-red', target, library)
        assert result.returncode == 101 and 'test result: FAILED' in result.stdout and label in result.stdout and 'could not compile' not in result.stdout, result.stdout
    finally:
        path.write_bytes(original[path])
        assert path.read_bytes() == original[path]
    restored = run(name + '-restored', target, library)
    assert restored.returncode == 0 and '1 passed' in restored.stdout, restored.stdout
    print(name + ': compiled named RED -> exact-restored GREEN', flush=True)

# Separate reachability control: the inert builder can be poisoned without
# changing the constructed live legacy REST/Store witness. Its direct unit
# identity witness is intentionally not selected for this production census.
needle = '    if let CoinbaseStarbaseWriteAmountIntent::Contracts(_) = &intent.size {'
mutant = replaced(starbase, needle, '    if std::hint::black_box(true) { panic!("inactive builder invoked by live placement"); }\n' + needle)
try:
    starbase.write_bytes(mutant)
    result = run('inert-builder-poison-live-green', live)
    assert result.returncode == 0 and '1 passed' in result.stdout, result.stdout
finally:
    starbase.write_bytes(original[starbase])
restored = run('inert-builder-restored', live)
assert restored.returncode == 0 and '1 passed' in restored.stdout, restored.stdout
print('inactive builder poison: live REST/Store remains GREEN; exact restore GREEN', flush=True)
for p, data in original.items():
    assert p.read_bytes() == data
    print(p.relative_to(root), hashlib.sha256(data).hexdigest(), flush=True)
