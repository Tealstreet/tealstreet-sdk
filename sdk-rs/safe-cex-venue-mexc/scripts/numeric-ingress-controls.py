"""Prove both MEXC numeric-safe entry points detect legacy decoder regressions."""
from pathlib import Path
import subprocess

crate = Path(__file__).resolve().parents[1]
types = crate / 'src/types.rs'
command = [
    'cargo', 'test', '--manifest-path', 'sdk-rs/Cargo.toml', '--locked',
    '-p', 'safe-cex-venue-mexc', '--test', 'numeric_ingress', '--jobs', '2',
    '--', '--exact', 'mexc_envelope_numeric_ingress_matches_node', '--nocapture',
]
controls = [
    ('checked Value -0',
     [('    js_from_value(value)\n', '    serde_json::from_value(value)\n'),
      ('JsNumberOutput, js_from_str, js_from_value', 'JsNumberOutput, js_from_str')]),
]

for assertion, replacements in controls:
    original = types.read_text()
    mutated = original
    for before, after in replacements:
        assert mutated.count(before) == 1, f'{assertion}: unique source mutation anchor {before!r}'
        mutated = mutated.replace(before, after)
    try:
        types.write_text(mutated)
        result = subprocess.run(
            command, cwd=crate.parents[1], text=True,
            stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
        )
        assert result.returncode == 101, result.stdout
        assert 'mexc_envelope_numeric_ingress_matches_node ... FAILED' in result.stdout, result.stdout
        assert assertion in result.stdout, result.stdout
        print(f'Rust decoder mutation: {assertion} fails at 101', flush=True)
    finally:
        types.write_text(original)

subprocess.run(command, cwd=crate.parents[1], check=True)
