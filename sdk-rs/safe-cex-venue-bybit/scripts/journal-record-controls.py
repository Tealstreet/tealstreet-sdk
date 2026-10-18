#!/usr/bin/env python3
"""Serial native adapter controls with exact source restoration."""
import os
from pathlib import Path
import subprocess
import tempfile

root = Path(__file__).resolve().parents[3]
sdk = root / 'sdk-rs'
source = sdk / 'safe-cex-venue-bybit/src/journal_fold.rs'
original_bytes = source.read_bytes()
original = original_bytes.decode()
command = ['cargo','test','-p','safe-cex-venue-bybit','--lib','journal_records_match_source_native_values','--jobs','1','--','--include-ignored']
compiler = subprocess.check_output(['node','-p',"require.resolve('typescript')"],cwd=sdk,text=True).strip()
env = dict(os.environ, SAFE_CEX_TYPESCRIPT_PATH=os.environ.get('SAFE_CEX_TYPESCRIPT_PATH',compiler))
logs = Path(tempfile.mkdtemp(prefix='bybit-journal-controls-'))
controls = [
    ('eager-optional-leg', 'if callback.is_nullish() {\n        Ok(Js::Undefined)', 'if callback.is_nullish() {\n        args()?;\n        Ok(Js::Undefined)'),
    ('finite-num-zero', 'crate::ledger_mapping::to_number(value)', 'crate::ledger_mapping::to_number(value).map(|_| 0.)'),
    ('closing-side-inversion', '"long"\n    } else {\n        "short"', '"short"\n    } else {\n        "long"'),
    ('funding-sign', 'net_pnl + fees - funding.to_number()?', 'net_pnl + fees + funding.to_number()?'),
    ('partial-output', 'Ok(Js::array(records))', 'records.truncate(1);\n    Ok(Js::array(records))'),
    ('inherited-fallback', '*object.proto.borrow_mut() = Some(Js::Builtin(\n            safe_cex_num::PrototypeRealm::current().reference(safe_cex_num::ProtoMember::Prototype),\n        ));', '*object.proto.borrow_mut() = None;'),
]
def run(name):
    result = subprocess.run(command,cwd=sdk,env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,timeout=180)
    (logs / (name+'.log')).write_text(result.stdout)
    return result
try:
    for name,before,after in controls:
        assert original.count(before) == 1,(name,'mutation selector drift')
        source.write_text(original.replace(before,after))
        result = run(name)
        assert result.returncode == 101 and 'panicked at' in result.stdout and 'journal_records_match_source_native_values ... FAILED' in result.stdout,(name,'SURVIVOR or non-runtime failure',str(logs))
        print(name,'compiled runtime FAIL',flush=True)
finally:
    source.write_bytes(original_bytes)
assert source.read_bytes() == original_bytes
result = run('restored')
assert result.returncode == 0 and '1 passed; 0 failed; 0 ignored' in result.stdout,('restored failure',str(logs))
print('exact restored source PASS; logs',logs,flush=True)
