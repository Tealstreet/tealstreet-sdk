#!/usr/bin/env python3
"""Each real Scope receiver must detect a compiled semantic body corruption."""
from pathlib import Path
import json
import subprocess
import tempfile

sdk = Path(__file__).resolve().parents[2]
paths = {name: sdk / 'safe-cex-venue-kraken/src/binding' / name for name in ['host.rs','host_typed.rs']}
originals = {name: path.read_bytes() for name,path in paths.items()}
receipts = Path(tempfile.mkdtemp(prefix='kraken-runtime-receiver-controls-'))
cases = [
    ('host.rs','iterable','iterable_uses_native_collection_protocol_and_retains_cells','if !method.is_callable()', 'if !matches!(value, Js::Array(_) | Js::String(_))'),
    ('host.rs','send','send_uses_actual_clients_verbs_wire_and_native_response','Client::Unlimited => "unlimitedXHR"','Client::Unlimited => "xhr"'),
    ('host_typed.rs','emit_tick_error','emit_tick_error_uses_message_or_fallback_through_real_base','fallback.to_owned()','format!("{fallback} wrong")'),
]
results = []
def run(name,label,red):
    command=['cargo','test','--locked','--jobs','1','-p','safe-cex-venue-kraken','--lib',name,'--','--exact','--nocapture']
    result=subprocess.run(command,cwd=sdk,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
    (receipts / f'{label}.log').write_text(result.stdout)
    assert 'could not compile' not in result.stdout,result.stdout
    if red:
        assert result.returncode != 0 and f'test {name} ... FAILED' in result.stdout and 'panicked at' in result.stdout,result.stdout
    else:
        assert result.returncode == 0 and f'test {name} ... ok' in result.stdout,result.stdout
    return result.returncode
try:
    for file,method,test,needle,replacement in cases:
        text=originals[file].decode()
        start=text.index(f'    fn {method}(')
        end=text.find('\n    fn ',start+1)
        if end==-1: end=text.index('\n}\n',start)
        body=text[start:end]
        assert body.count(needle)==1,(file,method,needle)
        name=f'binding::exchange::accepted_runtime_receivers::{test}'
        try:
            paths[file].write_text(text[:start]+body.replace(needle,replacement)+text[end:])
            red=run(name,f'{method}-red',True)
        finally:
            paths[file].write_bytes(originals[file])
        assert paths[file].read_bytes()==originals[file]
        green=run(name,f'{method}-restored',False)
        results.append({'method':method,'test':name,'red':red,'restored':green})
        (receipts/'receipts.json').write_text(json.dumps(results,indent=2)+'\n')
        print(f'{method}: named compiled RED, exact restoration GREEN',flush=True)
finally:
    for file,path in paths.items(): path.write_bytes(originals[file])
print(receipts,flush=True)
