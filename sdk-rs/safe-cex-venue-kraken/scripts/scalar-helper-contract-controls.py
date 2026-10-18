#!/usr/bin/env python3
"""Six compiling scalar defects, actual native/production failures, exact restores."""
import os
from pathlib import Path
import subprocess

workspace = Path(__file__).resolve().parents[2]
crate = workspace / 'safe-cex-venue-kraken'
runner = crate / 'scripts/scalar-helper-contracts.mjs'
originals = {name: (crate / 'src' / name).read_bytes() for name in ('fills.rs', 'income.rs')}
controls = [
    ('fills::num', 'fills.rs',
     b'Ok(if parsed.is_finite() { parsed } else { 0.0 })',
     b'Ok(if parsed.is_finite() { parsed + 1.0 } else { 0.0 })',
     'fills_num_preserves_finite_bits_and_uses_positive_zero_fallback'),
    ('fills::optional_num', 'fills.rs', b'if value.is_none_or(|v| v.is_null() || v.as_str() == Some("")) {\n        return Ok(None);\n    }',
     b'if value.is_none_or(|v| v.is_null() || v.as_str() == Some("")) {\n        return Ok(Some(0.0));\n    }',
     'fills_optional_num_observes_presence_before_native_finite_bits'),
    ('fills::to_seconds', 'fills.rs', b'(ms / 1000.0).floor()', b'(ms / 1000.0).ceil()',
     'fills_to_seconds_floors_before_epoch_and_uses_explicit_local_host'),
    ('income::num', 'income.rs', b'if value.is_none_or(|v| v.is_null() || v.as_str() == Some("")) {\n        return Ok(None);\n    }',
     b'if value.is_none_or(|v| v.is_null() || v.as_str() == Some("")) {\n        return Ok(Some(0.0));\n    }',
     'income_num_observes_presence_before_native_finite_bits'),
    ('income::to_ms', 'income.rs',
     b'Ok(if parsed.is_finite() { parsed } else { 0.0 })',
     b'Ok(if parsed.is_finite() { parsed / 1000.0 } else { 0.0 })',
     'income_to_ms_keeps_milliseconds_and_uses_explicit_local_host'),
    ('income::is_position_size_leg', 'income.rs',
     b'Ok(!asset.is_empty() && asset == contract)', b'Ok(asset.is_empty() && asset == contract)',
     'position_leg_decodes_fields_before_nonempty_case_insensitive_comparison'),
]


def cargo(test=None):
    args = ['cargo', 'test', '--locked', '-p', 'safe-cex-venue-kraken', '--jobs', '1', '--test', 'accepted_scalar_helpers']
    args += [test, '--', '--exact', '--nocapture'] if test else ['--test', 'utilities', '--', '--include-ignored']
    result = subprocess.run(args, cwd=workspace, env={**os.environ, 'RUSTC_BOOTSTRAP': '1'},
                            capture_output=True, text=True, timeout=900)
    return result, result.stdout + result.stderr


def production(owner=None):
    args = ['node', str(runner)]
    if owner:
        args.append('--expect-output-failure=' + owner)
    result = subprocess.run(args, cwd=workspace, capture_output=True, text=True, timeout=900)
    return result, result.stdout + result.stderr


def restored_production():
    result, output = production()
    assert result.returncode == 0 and '6 production runtime results; 0 explicit classifications; 0 failed owners' in output, output[-10000:]
    print('RESTORED PASS: all six actual production output probes', flush=True)


baseline, output = cargo()
assert baseline.returncode == 0 and output.count('6 passed; 0 failed; 0 ignored') == 2, output[-10000:]
print('BASELINE PASS: native6 + actual-source utilities6, zero ignored', flush=True)
restored_production()
try:
    for short_owner, name, old, new, test in controls:
        owner = 'safe_cex_venue_kraken::' + short_owner
        source = crate / 'src' / name
        original = originals[name]
        assert original.count(old) == 1, (owner, 'mutation anchor must identify exactly one body')
        try:
            source.write_bytes(original.replace(old, new))
            changed, output = cargo(test)
            assert (changed.returncode not in (0, 126) and f'test {test} ... FAILED' in output
                    and 'test result: FAILED' in output and 'could not compile' not in output), output[-10000:]
            print(f'CONTROL PASS: {owner}; named native witness fails after compilation', flush=True)
            changed, output = production(owner)
            assert changed.returncode == 0 and f'NATIVE_OUTPUT_FAILURE: {owner}' in output, output[-10000:]
            print(f'CONTROL PASS: {owner}; only its compiled production output mismatches', flush=True)
        finally:
            for file, contents in originals.items():
                (crate / 'src' / file).write_bytes(contents)
        restored, output = cargo(test)
        assert restored.returncode == 0 and f'test {test} ... ok' in output and '1 passed; 0 failed; 0 ignored' in output, output[-10000:]
        print(f'RESTORED PASS: {owner}; actual named native witness', flush=True)
        restored_production()
finally:
    for name, contents in originals.items():
        (crate / 'src' / name).write_bytes(contents)

restored, output = cargo()
assert restored.returncode == 0 and output.count('6 passed; 0 failed; 0 ignored') == 2, output[-10000:]
assert all((crate / 'src' / name).read_bytes() == contents for name, contents in originals.items())
print('FINAL PASS: exact source restored; native6 + actual-source utilities6, zero ignored', flush=True)
