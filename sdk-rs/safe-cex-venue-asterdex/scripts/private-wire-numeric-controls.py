#!/usr/bin/env python3
"""Measure actual lexical decoders in copied production; never edit live Rust."""
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[3]
SDK = ROOT / 'sdk-rs'
WORK = SDK / 'target/asterdex-private-wire-workspace'
LOGS = Path(tempfile.mkdtemp(prefix='asterdex-private-wire-'))
FIXTURES = json.loads((SDK / 'safe-cex-venue-asterdex/tests/fixtures/private-wire-numeric.json').read_text())
assert len(FIXTURES) == 6
assert len(os.sys.argv) == 1, 'numeric audit takes no scope-changing arguments'

source = subprocess.run(['node', str(Path(__file__).with_name('private-wire-numeric-source.cjs'))],
                        cwd=ROOT, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
assert source.returncode == 0, source.stdout.decode(errors='replace')
print(source.stdout.decode().strip(), flush=True)
if WORK.exists():
    shutil.rmtree(WORK)
WORK.mkdir(parents=True)
ignore = shutil.ignore_patterns('target', 'node_modules', '.git')
shutil.copytree(SDK, WORK / 'sdk-rs', ignore=ignore)
shutil.copytree(ROOT / 'infra/tradetunnel', WORK / 'infra/tradetunnel', ignore=ignore)
(WORK / 'packages').symlink_to(ROOT / 'packages', target_is_directory=True)
types = WORK / 'sdk-rs/safe-cex-venue-asterdex/src/types.rs'
original = types.read_bytes()
live_types = SDK / 'safe-cex-venue-asterdex/src/types.rs'
assert original == live_types.read_bytes()
print(f'Production source SHA256 {hashlib.sha256(original).hexdigest()}', flush=True)
env = {**os.environ, 'CARGO_TARGET_DIR': str(SDK / 'target/accepted-set-build'), 'CARGO_BUILD_JOBS': '1'}
command = [os.environ.get('CARGO', 'cargo'), 'test', '--manifest-path', str(WORK / 'sdk-rs/Cargo.toml'),
           '--locked', '--jobs', '1', '-p', 'safe-cex-venue-asterdex', '--test', 'private_wire_numeric']


def run(label, failed=0, selected=None, markers=(), measure=False):
    count = 1 if selected else 6
    args = [*command, '--', '--nocapture', *(['--exact', selected] if selected else [])]
    result = subprocess.run(args, cwd=WORK, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    output = result.stdout.decode(errors='replace')
    (LOGS / f'{label}.log').write_text(output)
    if measure:
        assert result.returncode in [0, 101], f'{label}: exit {result.returncode}; {LOGS}'
        failed = int(result.returncode == 101)
    else:
        assert result.returncode == (101 if failed else 0), f'{label}: exit {result.returncode}; {LOGS}'
    assert 'Finished `test`' in output and 'error[E' not in output, f'{label}: compiled execution required; {LOGS}'
    assert f'running {count} test' in output and f'{count-failed} passed; {failed} failed' in output, f'{label}: test census; {LOGS}'
    expected = markers if not measure or failed else [
        f'ASTER PRIVATE Wire {label} integer/decimal/-0 native fields verified']
    for marker in expected:
        assert marker in output, f'{label}: absent assertion {marker}; {LOGS}'
    if measure and failed:
        assert '1.25:' in output or '-0:' in output, f'{label}: named numeric failure required; {LOGS}'
    print(f'{label}: exit {result.returncode}; {count-failed} pass/{failed} fail', flush=True)
    return failed


def point(text, owner):
    start = text.index(f"impl<'de> serde::Deserialize<'de> for {owner} {{")
    wire = text.index('        struct Wire {', start)
    end = text.index('\n        }\n', wire) + len('\n        }\n')
    fields = re.findall(r'^            ((?:r#)?[A-Za-z_][A-Za-z0-9_]*):', text[wire:end], re.M)
    assert fields and len(fields) == len(set(fields))
    assert FIXTURES[owner]['number'] in fields and FIXTURES[owner]['tag'] in fields
    return end, fields


def number(variable, row):
    return f'{variable}.{row["number"]}' + ('.expect("present numeric field")' if row['optional'] else '')


def diagnostic(owner, row):
    return '''
        {
            let fixtures: serde_json::Value = serde_json::from_str(include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/private-wire-numeric.json"))).unwrap();
            let row = &fixtures["OWNER"];
            let template = row["input"].to_string();
            let integer: Wire = serde_json::from_str(&template.replace("\\\"__number__\\\"", "1")).expect("ASTER PRIVATE Wire OWNER integer baseline");
            assert_eq!(INTEGER.to_bits(), 0x3ff0_0000_0000_0000, "ASTER PRIVATE Wire OWNER integer native bits");
            assert_eq!(integer.TAG.as_str(), row["tagExpected"].as_str().unwrap(), "ASTER PRIVATE Wire OWNER native tag");
            assert_eq!(integer.extra["opaqueTag"], row["input"]["opaqueTag"], "ASTER PRIVATE Wire OWNER native extension");
            let mut failures = Vec::new();
            for (literal, bits) in [("1.25", 0x3ff4_0000_0000_0000), ("-0", 0x8000_0000_0000_0000)] {
                match serde_json::from_str::<Wire>(&template.replace("\\\"__number__\\\"", literal)) {
                    Ok(value) => if VALUE.to_bits() != bits { failures.push(format!("{literal}: wrong native bits")); },
                    Err(error) => failures.push(format!("{literal}: {error}")),
                }
            }
            assert!(failures.is_empty(), "ASTER PRIVATE Wire OWNER direct decimal/-0 admission: {}", failures.join("; "));
            eprintln!("ASTER PRIVATE Wire OWNER integer/decimal/-0 native fields verified");
        }
'''.replace('OWNER', owner).replace('INTEGER', number('integer', row)).replace('VALUE', number('value', row)).replace('TAG', row['tag'])


try:
    run('public-baseline')
    outcomes = {}
    for owner, row in FIXTURES.items():
        text = original.decode()
        at, _ = point(text, owner)
        types.write_text(text[:at] + diagnostic(owner, row) + text[at:])
        outcomes[owner] = run(owner, selected=row['test'], measure=True,
            markers=[f'ASTER PRIVATE Wire {owner} direct decimal/-0 admission'])
        types.write_bytes(original)
        run(f'{owner}-restored')
    (LOGS / 'private-outcomes.json').write_text(json.dumps(outcomes, indent=2) + '\n')
    assert not any(outcomes.values()), f'measured private numeric failures need repair: {outcomes}; {LOGS}'
    instrumented = original.decode()
    for owner in reversed(FIXTURES):
        at, _ = point(instrumented, owner)
        instrumented = instrumented[:at] + diagnostic(owner, FIXTURES[owner]) + instrumented[at:]
    types.write_text(instrumented)
    run('private-fields-baseline')
    start = instrumented.index("impl<'de> serde::Deserialize<'de> for AsterDEXWebSocketMessage {")
    end = instrumented.index('\n}\n', start) + 3
    region = instrumented[start:end]
    needle = 'capital_e: w.capital_e,'
    assert region.count(needle) == 1
    types.write_text(instrumented[:start] + region.replace(needle, 'capital_e: 0.0,') + instrumented[end:])
    run('public-field-bit-loss', failed=1,
        markers=['ASTER PUBLIC AsterDEXWebSocketMessage text 1: numeric ingress changed Number bits at $.E'])
    types.write_text(instrumented)
    run('public-field-bit-loss-restored')
finally:
    types.write_bytes(original)
assert types.read_bytes() == live_types.read_bytes() == original
print(f'Six private numeric decoders pass; compiled bit-loss caught/restored; live Rust untouched; logs {LOGS}')
