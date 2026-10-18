#!/usr/bin/env python3
"""Each concrete receiver must detect its own compiled semantic fault."""
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile

sdk = Path(__file__).resolve().parents[2]
crate = sdk / 'safe-cex-venue-kraken'
receipts = Path(tempfile.mkdtemp(prefix='kraken-additional-receiver-controls-'))
cases = [
    ('post_cancel', 'binding/host_typed.rs', 'impl SpotCancelHost for Rt',
     'post_cancel_uses_the_signed_spot_client_and_original_id',
     'Js::object(vec![(field, (*id).clone())])', 'Js::object(vec![(if field == "txid" { "wrong" } else { field }, (*id).clone())])'),
    ('error_message', 'binding/host_typed.rs', 'impl SpotCancelHost for Rt',
     'spot_error_message_keeps_truthy_text_and_falsy_fallback',
     'if message.truthy()', 'if !message.truthy()'),
    ('emit_error', 'binding/host_typed.rs', 'impl SpotCancelHost for Rt',
     'spot_emit_error_reaches_the_real_base_reporter',
     'vec![message.clone(), Js::str(context)]', 'vec![message.clone(), Js::str(if context.is_empty() { context } else { "wrong context" })]'),
    ('position_id', 'binding/host_more.rs', 'impl AccountHost for Rt',
     'position_id_keeps_native_utf16_and_coercion_errors', 'units(":")', 'units("-")'),
    ('optional_preference', 'binding/host_more.rs', 'impl AccountHost for Rt',
     'optional_preference_uses_parent_maps_and_original_keys',
     'map.call_method("get", vec![symbol.clone()])?', 'map.call_method("has", vec![symbol.clone()])?'),
    ('imported', 'binding/members.rs', 'impl ClassHost for Rt',
     'imported_values_match_the_constructed_spot_endpoint_tables',
     '"GRAFANA_ENDPOINTS_EVERY" => crate::spot_types::GRAFANA_ENDPOINTS_EVERY',
     '"GRAFANA_ENDPOINTS_EVERY" => crate::spot_types::GRAFANA_ENDPOINTS_PERIODIC'),
    ('member', 'binding/members.rs', 'impl ClassHost for Rt',
     'member_dispatch_uses_the_supplied_receiver_and_argument_cells',
     'dispatch(rt.with_this((*receiver).clone()), target, args)', 'dispatch(rt.with_this((*receiver).clone()).with_this(rt.this.clone()), target, args)'),
    ('prototype', 'binding/members.rs', '    fn prototype(',
     'prototype_installs_native_this_args_and_future_on_the_matching_socket_kind',
     'body(h(this), args)?', 'body(h(if this.is_nullish() { this } else { Js::Undefined }), args)?'),
    ('retained-message', 'spot_cancel.rs', 'pub async fn cancel_orders',
     'spot_cancel_preserves_the_caught_utf16_message_before_rethrow',
     'Js::utf16(&message.to_js_string()?)', 'Js::str(&message.to_display())'),
]
results = []
for method, filename, start_at, test, original_text, broken_text in cases:
    source = crate / 'src' / filename
    original = source.read_bytes()
    text = original.decode()
    start = text.index(start_at)
    prefix, body = text[:start], text[start:]
    assert body.count(original_text) == 1, (method, original_text)
    name = f'binding::exchange::accepted_additional_receivers::{test}'
    command = ['cargo','test','--locked','--jobs','1','-p','safe-cex-venue-kraken','--lib',name,'--','--exact','--nocapture']
    def run(label, red):
        result = subprocess.run(command, cwd=sdk, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        log = receipts / f'{method}-{label}.log'
        log.write_text(result.stdout)
        if red:
            assert result.returncode != 0 and f'test {name} ... FAILED' in result.stdout and 'panicked at' in result.stdout and 'could not compile' not in result.stdout, result.stdout
        else:
            assert result.returncode == 0 and f'test {name} ... ok' in result.stdout, result.stdout
        return {'exit': result.returncode, 'log':str(log)}
    try:
        source.write_text(prefix + body.replace(original_text,broken_text))
        red = run('red', True)
    finally:
        source.write_bytes(original)
    assert source.read_bytes() == original
    green = run('restored', False)
    results.append({'method':method,'sourceSha256':hashlib.sha256(original).hexdigest(),'red':red,'restored':green})
    (receipts/'receipts.json').write_text(json.dumps(results,indent=2)+'\n')
    print(f'{method}: compiled named RED, exact restoration GREEN',flush=True)
print(receipts,flush=True)
