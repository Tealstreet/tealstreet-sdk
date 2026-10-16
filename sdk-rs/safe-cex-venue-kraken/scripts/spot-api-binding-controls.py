"""Compile each native API-host mutant, require its named test to fail, restore.

Run alone: this temporarily edits production source and always restores bytes.
"""
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[3]
SRC = ROOT / 'sdk-rs/safe-cex-venue-kraken/src/binding/host.rs'
CONTROLS = [
    ('spot_retry_condition', 'Js::Bool(true))?;', 'Js::Bool(false))?;',
     'spot_repeated_signature_rejection_stops_after_one_resend'),
    ('qs_parse', 'Ok(h(qs_parse_form(&value)?))',
     'Ok(h({ let result = qs_parse_form(&value)?; result.delete(&units("volume")); result }))',
     'spot_order_resend_retains_the_unsigned_order_fields'),
    ('resend', 'client.call_method("request",', 'client.call_method("get",',
     'spot_signature_rejection_replays_the_signed_form_once'),
]


def run(test):
    return subprocess.run(
        ['cargo', 'test', '--manifest-path', str(ROOT / 'sdk-rs/Cargo.toml'),
         '--locked', '-p', 'safe-cex-venue-kraken', '--test', 'exchange_binding',
         '--jobs', '1', test, '--', '--exact'],
        cwd=ROOT, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)


original = SRC.read_bytes()
try:
    for method, before, after, test in CONTROLS:
        baseline = run(test)
        assert baseline.returncode == 0 and '1 passed' in baseline.stdout, baseline.stdout
        text = original.decode()
        start = text.index('    fn ' + method + '(')
        end = text.find('\n    fn ', start + 1)
        end = len(text) if end < 0 else end
        body = text[start:end]
        assert body.count(before) == 1, (method, body)
        SRC.write_text(text[:start] + body.replace(before, after) + text[end:])
        mutant = run(test)
        assert mutant.returncode == 101 and f'test {test} ... FAILED' in mutant.stdout, mutant.stdout
        SRC.write_bytes(original)
        restored = run(test)
        assert restored.returncode == 0 and '1 passed' in restored.stdout, restored.stdout
        print(f'{method}: runtime failure observed; restored 1 passed', flush=True)
finally:
    SRC.write_bytes(original)
