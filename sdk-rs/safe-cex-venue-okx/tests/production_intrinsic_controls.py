"""Restore each production regression, require its named runtime failure, then pass."""
from pathlib import Path
import subprocess

sdk = Path(__file__).resolve().parents[2]
source = sdk / 'safe-cex-venue-okx/src/binding/intrinsics.rs'
original = source.read_bytes()
logs = sdk / 'target/okx-production-intrinsic-controls'
logs.mkdir(parents=True, exist_ok=True)
base = ['cargo', 'test', '--locked', '--jobs', '1', '-p', 'safe-cex-venue-okx']
spread = '''            let mut out = Vec::new();
            for value in a {
                out.extend(value.iterable_values()?);
            }'''
old_spread = '''            let mut out = x.iterable_values()?;
            if a.len() > 1 {
                out.extend(y.iterable_values()?);
            }'''
colon = '''        "colon-append" => {
            let mut out = x.to_js_string()?.to_vec();
            out.push(58);
            out.extend_from_slice(&y.to_js_string()?);
            Js::utf16(&out)
        }'''
old_colon = '''        "colon-append" => Js::str(&format!("{}:{}", text(&x)?, text(&y)?)),'''
concat = '''        "template-concat" | "template-suffix" => {
            let mut out = Vec::new();
            for part in &a {
                out.extend_from_slice(&part.to_js_string()?);
            }
            Js::utf16(&out)
        }'''
old_concat = '''        "template-concat" | "template-suffix" => Js::str(&(text(&x)? + &text(&y)?)),'''
is_array = '''        "is-array" => Js::Bool(x.is_array()),
'''
string = '''        "string" => Js::utf16(&x.to_js_string()?),
'''
add = '''        "add" => ops::add(&x, &y)?,
'''
controls = [
    ('orders', spread, old_spread, 'order_reads', 'order_reads_native_before_source', 'ORDER native independent result count'),
    ('cancellation', spread, old_spread, 'exchange_binding', 'fetch_orders_retains_all_five_groups_for_symbol_cancellation', 'OKX production retains all five order groups in source order'),
    ('subscriptions', spread, old_spread, 'websocket_account_events', 'market_events_native', 'MARKETS native 100-topic batch widths'),
    ('utf16-dedupe', colon, old_colon, 'websocket_account_events', 'dedupe_native', 'maintained source dedupe/4'),
    ('error-is-array', is_array, '', 'exchange_binding', 'okx_place_rejection_reads_the_attached_response', 'unknown OKX intrinsic is-array'),
    ('error-string', string, '', 'exchange_binding', 'okx_api_error_without_msg_names_the_full_code', 'unknown OKX intrinsic string'),
    ('error-fallback-concat', concat, old_concat, 'exchange_binding', 'okx_api_error_without_msg_names_the_full_code', '"OKX API error (code 50011"'),
    ('error-row-concat', concat, old_concat, 'exchange_binding', 'okx_api_error_row_message_carries_its_code', '"Insufficient margin (code "'),
    ('income-add', add, '', 'exchange_binding', 'fetch_income_history_maps_a_nonempty_bills_page', 'unknown OKX intrinsic add'),
]


def run(target, test, log):
    result = subprocess.run(base + ['--test', target, test, '--', '--exact'], cwd=sdk,
                            text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    (logs / log).write_text(result.stdout)
    return result


for label, site, replacement, target, test, assertion in controls:
    if source.read_bytes() != original:
        raise RuntimeError('production source changed between controls')
    text = original.decode()
    if text.count(site) != 1:
        raise RuntimeError(f'{label}: expected one production site')
    try:
        source.write_text(text.replace(site, replacement))
        red = run(target, test, f'{label}-red.log')
        if (red.returncode != 101 or f'test {test} ... FAILED' not in red.stdout
                or assertion not in red.stdout or 'error[E' in red.stdout):
            raise RuntimeError(f'{label}: expected named compiled RED, got {red.returncode}\n{red.stdout}')
    finally:
        source.write_bytes(original)
    green = run(target, test, f'{label}-restored.log')
    if green.returncode != 0 or '1 passed; 0 failed' not in green.stdout:
        raise RuntimeError(f'{label}: restored target failed\n{green.stdout}')
    print(f'{label}: compiled named RED 101; byte-restored PASS', flush=True)
assert source.read_bytes() == original
print(f'{len(controls)} controls passed', flush=True)
