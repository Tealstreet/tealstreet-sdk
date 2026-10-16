"""Sequential compiling controls; restore the candidate after every mutation."""
from pathlib import Path
import os
import subprocess
import sys

source = Path('sdk-rs/safe-cex-venue-woo/src/cancel_all_after.rs')
original = source.read_text()
cases = [
    ('utf16-keys', 'expected.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));', 'expected.sort_unstable();', 'exact key UTF-16 diagnostic order'),
    ('ceiling', '(0.0..=900_000.0).contains(n)', '(0.0..=900_001.0).contains(n)', 'cancel-after exact validation priority'),
    ('timestamp-swap', '"timestamp": timestamp, "expectedTriggerTime": expected_trigger_time', '"timestamp": expected_trigger_time, "expectedTriggerTime": timestamp', 'response timestamp native field identity'),
    ('negative-zero', '"triggerAfter": trigger_after', '"triggerAfter": if trigger_after == 0.0 { 0.0 } else { trigger_after }', 'triggerAfter native bits'),
    ('success-truthiness', 'response.get("success") != Some(&Value::Bool(true))', '!safe_cex_num::js_truthy(response.get("success"))', 'cancel-after exact validation priority'),
    ('data-key-guard', 'require_exact_keys(data, &["expectedTriggerTime"], &format!("{path}.data"))?;', '', 'cancel-after exact validation priority'),
]
env = dict(os.environ)
env['PATH'] = '/opt/homebrew/opt/rustup/bin:' + env['PATH']
for name, before, after, assertion in cases:
    if len(sys.argv) > 1 and name not in sys.argv[1:]:
        continue
    assert original.count(before) == 1, name
    try:
        source.write_text(original.replace(before, after))
        run = subprocess.run(['cargo', 'test', '--manifest-path', 'sdk-rs/Cargo.toml', '--locked', '--offline', '--jobs', '1', '-p', 'safe-cex-venue-woo', '--test', 'cancel_all_after'], env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        Path('/tmp/woo-cancel-after-control-' + name + '.log').write_text(run.stdout)
        valid = run.returncode == 101 and 'test result: FAILED' in run.stdout and assertion in run.stdout
        print(name, run.returncode, 'assertion-red' if valid else 'INVALID', flush=True)
        assert valid, run.stdout
    finally:
        source.write_text(original)
