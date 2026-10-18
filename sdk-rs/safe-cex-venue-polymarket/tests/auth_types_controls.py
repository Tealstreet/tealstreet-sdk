"""Sequential, reversible mutations of this slice; every expected failure is named."""
import os
from pathlib import Path
import subprocess

crate = Path(__file__).resolve().parents[1]
repo = crate.parents[1]
rust = crate / 'src/types.rs'
oracle = crate / 'tests/auth_types_oracle.cjs'
source = repo / 'packages/safe-cex/src/exchanges/polymarket/polymarket.types.ts'
env = dict(os.environ)
log_directory = Path(env['POLYMARKET_CONTROL_LOG_DIR']) if env.get('POLYMARKET_CONTROL_LOG_DIR') else None
if log_directory:
    log_directory.mkdir(parents=True, exist_ok=True)

def save_log(name, result):
    if log_directory:
        (log_directory / (name.replace(' ', '-') + '.log')).write_text(f'exit_code={result.returncode}\n' + result.stdout)

assert env.get('SAFE_CEX_TYPESCRIPT_PATH'), 'SAFE_CEX_TYPESCRIPT_PATH is required'
command = ['/opt/homebrew/opt/rustup/bin/cargo', 'test', '--manifest-path', str(repo / 'sdk-rs/Cargo.toml'), '-p', 'safe-cex-venue-polymarket', '--test', 'auth_types', '--locked', '--jobs', '1', '--', '--nocapture']


def run():
    result = subprocess.run(command, cwd=repo, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    assert result.stdout.count('polymarket auth oracle: one Node invocation for the entire corpus') == 1, 'one subprocess per corpus\n' + result.stdout
    return result


def mutate(label, file, change, assertion):
    original = file.read_text()
    changed = change(original)
    assert changed != original, label + ': mutation did not apply'
    try:
        file.write_text(changed)
        result = run()
        save_log(label + '-mutation', result)
        assert result.returncode == 101 and assertion in result.stdout, label + ': wrong failure\n' + result.stdout
        print(f'{label}: FAIL {assertion}', flush=True)
    finally:
        file.write_text(original)
    result = run()
    save_log(label + '-restored', result)
    assert result.returncode == 0, label + ': restoration failed\n' + result.stdout
    print(f'{label}: RESTORED PASS (7 tests)', flush=True)


def swap_record(text, name, first, second):
    start = text.index('pub struct ' + name)
    end = text.index('impl JsNumberOutput for ' + name, start)
    region = text[start:end]
    # Swap wire labels symmetrically in Serialize and Deserialize. A pure
    # roundtrip still agrees; direct native access must detect the corruption.
    if first == 'apiKey':
        region = region.replace('#[serde(rename = "apiKey")]', '#[serde(rename = "secret")]')
        region = region.replace('    pub secret: String,', '    #[serde(rename = "apiKey")]\n    pub secret: String,')
        region = region.replace('            secret: String,', '            #[serde(rename = "apiKey")]\n            secret: String,')
    else:
        region = region.replace('"' + first + '"', '"SWAP"').replace('"' + second + '"', '"' + first + '"').replace('"SWAP"', '"' + second + '"')
    return text[:start] + region + text[end:]

baseline = run()
save_log('auth-baseline', baseline)
assert baseline.returncode == 0, 'baseline must pass'
mutate('credentials symmetric swap', rust, lambda s: swap_record(s, 'PolymarketCredentials', 'apiKey', 'secret'), 'native_credentials_api_key')
mutate('response symmetric swap', rust, lambda s: swap_record(s, 'PolymarketDeriveKeyResponse', 'apiKey', 'secret'), 'native_response_api_key')
mutate('headers symmetric swap', rust, lambda s: swap_record(s, 'PolymarketL2Headers', 'POLY_ADDRESS', 'POLY_SIGNATURE'), 'native_headers_address')
mutate('numeric sign loss', rust, lambda s: s.replace('nonce: fields.nonce,', 'nonce: fields.nonce + 0.0,'), 'numeric ingress changed Number bits at $.nonce')
mutate('oracle sign loss', oracle, lambda s: s.replace('Number.isNaN(value) ? NaN : value)', 'Number.isNaN(value) ? NaN : value + 0)'), 'nonce_bits_before_json')
mutate('optional null collapse', rust, lambda s: s.replace('String::deserialize(d).map(Some)', 'Option::<String>::deserialize(d)'), 'present_null:PolymarketL2Headers.POLY_PASSPHRASE')
mutate('heritage blindness', oracle, lambda s: s.replace('type.getProperties().map(property =>', 'type.getProperties().filter(property => property.getDeclarations().every(d => d.parent === node)).map(property =>'), 'inherited_required_inventory')
mutate('actual source inherited field', source, lambda s: s.replace('export interface PolymarketCredentials {', 'interface RequiredCredentialParent { inheritedCredential: string }\nexport interface PolymarketCredentials extends RequiredCredentialParent {'), 'declared field inheritedCredential')
mutate('unresolved heritage', oracle, lambda s: s.replace('assert.equal(actualBases.length, expectedBases, `resolved_heritage ${node.name.text}`);', 'void actualBases; void expectedBases;'), 'unresolved_heritage_refusal')
mutate('typed surrogate corruption at ingress', crate / 'tests/auth_types.rs', lambda s: s.replace('    match name {', '    let normalized = raw.replace(r"\\ud800", r"corrupted");\n    let raw = normalized.as_str();\n    match name {', 1), 'typed auth U+FFFD field')
def swap_payload(text, name, first, second):
    start = text.index('pub struct ' + name)
    end = text.index('impl JsNumberOutput for ' + name, start)
    region = text[start:end]
    for field, wire in [(first, second), (second, first)]:
        region = region.replace('    pub ' + field + ': String,', '    #[serde(rename = "' + wire + '")]\n    pub ' + field + ': String,')
        region = region.replace('            ' + field + ': String,', '            #[serde(rename = "' + wire + '")]\n            ' + field + ': String,')
    return text[:start] + region + text[end:]

mutate('balance symmetric swap', rust, lambda s: swap_payload(s, 'PolymarketBalanceAllowance', 'balance', 'allowance'), 'native_balance.balance')
mutate('level symmetric swap', rust, lambda s: swap_payload(s, 'PolymarketOrderBookLevel', 'price', 'size'), 'native_level.price')
print('12 mutations rejected at named assertions; 12 restorations passed', flush=True)
