"""Local price slice controls: one corpus capture, named failure, restore."""
from pathlib import Path
import os
import subprocess

crate = Path(__file__).resolve().parents[1]
source = crate / 'src/types.rs'
original = source.read_text()
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH'), 'SAFE_CEX_TYPESCRIPT_PATH is required'
logs = Path(os.environ['POLYMARKET_CONTROL_LOG_DIR'])
logs.mkdir(parents=True, exist_ok=True)
command = ['/opt/homebrew/opt/rustup/bin/cargo', 'test', '--manifest-path', str(crate.parent / 'Cargo.toml'), '-p', 'safe-cex-venue-polymarket', '--test', 'auth_types', '--locked', '--jobs', '1', '--', 'price_record_actual_typescript', '--nocapture']

def run(name):
    result = subprocess.run(command, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    (logs / (name + '.log')).write_text(f'exit_code={result.returncode}\n' + result.stdout)
    assert result.stdout.count('polymarket auth oracle: one Node invocation for the entire corpus') == 1, result.stdout
    return result

def swap(text):
    start = text.index('pub struct PolymarketPrice')
    end = text.index('impl JsNumberOutput for PolymarketPrice', start)
    region = text[start:end]
    for field, wire in [('token_id', 'price'), ('price', 'token_id')]:
        region = region.replace('    pub ' + field + ': String,', '    #[serde(rename = "' + wire + '")]\n    pub ' + field + ': String,')
        region = region.replace('            ' + field + ': String,', '            #[serde(rename = "' + wire + '")]\n            ' + field + ': String,')
    return text[:start] + region + text[end:]

controls = [
    ('price-symmetric-swap', swap, 'native_price.token_id'),
    ('price-null-collapse', lambda s: s.replace('f64::deserialize(d).map(Some)', 'Option::<f64>::deserialize(d)'), 'present_null:PolymarketPrice.timestamp'),
    ('price-sign-loss', lambda s: s.replace('timestamp: fields.timestamp,\n        })\n    }\n}\nimpl JsNumberOutput for PolymarketPrice', 'timestamp: fields.timestamp.map(|value| value + 0.0),\n        })\n    }\n}\nimpl JsNumberOutput for PolymarketPrice'), 'numeric ingress changed Number bits at $.timestamp'),
]
assert run('baseline').returncode == 0
for name, mutate, assertion in controls:
    changed = mutate(original)
    assert changed != original, name + ': mutation did not apply'
    try:
        source.write_text(changed)
        result = run(name + '-mutation')
        assert result.returncode == 101 and assertion in result.stdout, result.stdout
        print(name + ': FAIL ' + assertion, flush=True)
    finally:
        source.write_text(original)
    assert run(name + '-restored').returncode == 0
    print(name + ': RESTORED PASS', flush=True)
