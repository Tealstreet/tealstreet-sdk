#!/usr/bin/env python3
"""Require compiled native account faults, then restore exact production bytes."""
import os
from pathlib import Path
import subprocess
import tempfile

sdk = Path(__file__).resolve().parents[2]
hyro = sdk / 'safe-cex-venue-bybit/src/binding/members/hyro.rs'
exchange = sdk / 'safe-cex-venue-bybit/src/exchange.rs'
value = sdk / 'safe-cex-exchange/src/js_runtime/value.rs'
receiver = 'hyro_divergence_account_configuration_receivers'
unit = 'ordinary_has_instance_matches_node_prototypes_bound_functions_and_primitives'
controls = [
    ('error-prototype', hyro,
     'body\n                                            .global("Error")?\n                                            .ordinary_has_instance(&error.value)?',
     'matches!(&error.value, Js::Object(data) if data.class == "Error")',
     receiver, 'hyro_error_prototype_message'),
    ('margin-disposal', hyro, 'if !owner.get("isDisposed")?.truthy() {',
     'if true {', receiver, 'hyro_margin_disposal_guard'),
    ('empty-income', hyro, 'Ok(Js::array(vec![])),\n                )))',
     'Ok(Js::array(vec![Js::Null])),\n                )))', receiver, 'hyro_empty_income_range'),
    ('option-guard', exchange,
     'self.base.name != safe_cex_types::ExchangeName::Hyrotrader\n            && self.base.options().enable_option_markets == Some(true)',
     'self.base.options().enable_option_markets == Some(true)',
     receiver, 'hyro_capability_flags_ignore_optins'),
    ('inverse-guard', exchange,
     'safe_cex_types::ExchangeName::Bybitspot | safe_cex_types::ExchangeName::Hyrotrader',
     'safe_cex_types::ExchangeName::Bybitspot', receiver, 'hyro_capability_flags_ignore_optins'),
    ('bound-target', value, 'return target.ordinary_has_instance(value);',
     'return target.ordinary_has_instance(&Js::Null);', unit, 'ordinary_has_instance_node_cases'),
]
env = dict(os.environ, CARGO_INCREMENTAL='0',
           NEXT_PUBLIC_HYROTRADER_PROD_REST_URL='https://hyro-audit.example',
           NEXT_PUBLIC_HYROTRADER_SIMULATOR_REST_URL='https://hyro-sim-audit.example')
assert not env.get('RUSTC_BOOTSTRAP'), 'use the workflow Cargo profile'
logs = Path(tempfile.mkdtemp(prefix='bybit-hyro-account-controls-'))
print(logs, flush=True)

def command(test):
    target = ['-p', 'safe-cex-exchange', '--test', 'js_construction'] if test == unit else ['-p', 'safe-cex-venue-bybit', '--lib']
    return ['cargo', 'test', '--locked', *target, test, '--jobs', '2', '--', '--test-threads=1']

def run(name, test, red=None):
    result = subprocess.run(command(test), cwd=sdk, env=env, text=True,
                            stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=600)
    path = logs / (name + '.log')
    path.write_text(result.stdout)
    if red:
        assert result.returncode and 'Finished `test`' in result.stdout, str(path)
        assert test + ' ... FAILED' in result.stdout and red in result.stdout, str(path)
    else:
        assert result.returncode == 0 and '1 passed; 0 failed' in result.stdout, str(path)
    print(name + ': ' + ('compiled named RED' if red else 'restored GREEN'), flush=True)

for name, path, before, after, test, label in controls:
    original = path.read_bytes()
    text = original.decode()
    assert text.count(before) == 1, (name, 'exact source anchor changed')
    try:
        path.write_text(text.replace(before, after))
        run(name, test, label)
    finally:
        path.write_bytes(original)
    assert path.read_bytes() == original
    run(name + '-restored', test)
