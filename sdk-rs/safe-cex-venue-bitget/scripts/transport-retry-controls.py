#!/usr/bin/env python3
"""Compile real Bitget host regressions against isolated production reversions."""
import hashlib, os, subprocess
from pathlib import Path
root = Path(__file__).resolve().parents[2]
source = root / 'safe-cex-venue-bitget/src/binding/hosts/assembly.rs'
original = source.read_bytes()
text = original.decode()
OLD_FACTORY = '    def(\n        "safeCexAxiosCreate",\n        Js::function("safeCexAxiosCreate", move |_, a| {\n            Ok(client::create(shared(&w)?.shared.http.clone(), arg(&a, 0)))\n        }),\n    );\n'
OLD_NETWORK = '    def(\n        "networkErrorOnRead",\n        Js::function("networkErrorOnRead", |_, _| {\n            Ok(Js::function("onlyReads", |_, a| {\n                let error = arg(&a, 0);\n                let config = error.get("config")?;\n                if config.is_undefined() {\n                    return Ok(Js::Bool(false));\n                }\n                let method = config.get("method")?;\n                let method = if method.is_nullish() {\n                    String::new()\n                } else {\n                    text(&method)?.to_lowercase()\n                };\n                if method != "get" && method != "head" {\n                    return Ok(Js::Bool(false));\n                }\n                let code = error.get("code")?;\n                Ok(Js::Bool(\n                    error.get("response")?.is_undefined()\n                        && code.truthy()\n                        && text(&code)? != "ECONNABORTED",\n                ))\n            }))\n        }),\n    );\n'
OLD_INSTALLER = '    def(\n        "installTransportAdapter",\n        Js::function("installTransportAdapter", |_, _| {\n            Err(Thrown::error(\n                "Error",\n                "Tealstreet tunnel transport is not available in the Rust Bitget binding",\n            ))\n        }),\n    );\n'

def replace_region(label, end_anchor, replacement, include_weak=False):
    start = text.index('    def(\n        "' + label + '",')
    if include_weak:
        prefix = '    let w = weak.clone();\n'
        assert text[start-len(prefix):start] == prefix
        start -= len(prefix)
    end = text.index(end_anchor, start)
    return text[:start] + replacement + text[end:]

def run(name=None):
    command = ['cargo', 'test', '--locked', '-p', 'safe-cex-venue-bitget', '--jobs', '1', '--test', 'transport_retry_binding']
    if name:
        command += [name, '--', '--exact', '--nocapture']
    return subprocess.run(command, cwd=root, env={**os.environ, 'RUSTC_BOOTSTRAP':'1'}, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)

constructor = 'real_tunnel_constructor_installs_provider_adapter_or_retains_host'
network = 'real_read_retry_matches_live_axios_exclusions_and_response_truthiness'
mutants = [
    ('provider-routing', constructor, 'real Axios factory must select provider routing', replace_region('safeCexAxiosCreate', '    def(\n        "retry",', OLD_FACTORY)),
    ('tunnel-installation', constructor, 'real tunnel constructor must succeed', replace_region('installTransportAdapter', '    def(\n        "qs.stringify",', OLD_INSTALLER, True)),
    ('network-deny-list', network, 'real network retry must match TS: "denied-ERR_CANCELED"', replace_region('networkErrorOnRead', '    // Keep the shared request stamp', OLD_NETWORK)),
]
anchor = 'if error.get("response")?.truthy() {'
assert text.count(anchor) == 1
mutants.append(('response-truthiness', network, 'real network retry must match TS: "response-null"', text.replace(anchor, 'if !error.get("response")?.is_undefined() {')))
baseline = run()
assert baseline.returncode == 0, baseline.stdout
print('BASELINE: real host tunnel/retry tests PASS; actual TS54retry/3installer scenes PASS', flush=True)
for name, target, label, mutant in mutants:
    assert source.read_bytes() == original
    try:
        source.write_text(mutant)
        result = run(target)
        assert result.returncode == 101 and 'test result: FAILED' in result.stdout and label in result.stdout, result.stdout
        print(result.stdout, flush=True)
    finally:
        source.write_bytes(original)
        assert source.read_bytes() == original
    restored = run(target)
    assert restored.returncode == 0, restored.stdout
    print('CONTROL ' + name + ': compiled named RED; exact-restored real host PASS', flush=True)
final = run()
assert final.returncode == 0, final.stdout
assert source.read_bytes() == original
print('RESTORED: both real-host tests PASS; source SHA256 ' + hashlib.sha256(original).hexdigest(), flush=True)
