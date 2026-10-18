"""Private field guard and borrowed-result controls, exact restoration."""
from pathlib import Path
import subprocess
SDK = Path(__file__).resolve().parents[2]
SOURCE = SDK / 'safe-cex-venue-cryptocom/src/ws_payloads.rs'
TEST = 'ws_payloads::accepted_ws_field::field_native_contract'
CONTROLS = [
    ('lookup', '.get(&key.encode_utf16().collect::<Vec<_>>())', '.get(&{ let _ = key; Vec::<u16>::new() })'),
    ('object guard', '_ => Err(PayloadError("object")),', '_ => Ok(&PayloadValue::Undefined),'),
]
def run():
    r = subprocess.run(['cargo','test','--locked','--jobs','1','-q','-p','safe-cex-venue-cryptocom','--lib',TEST,'--','--exact'],cwd=SDK,capture_output=True,text=True)
    return r.returncode,r.stdout+r.stderr
for name,before,after in CONTROLS:
    code,output=run(); assert code==0 and 'test result: ok. 1 passed' in output,output
    original=SOURCE.read_bytes();start=original.index(b"fn field<'a>");end=original.index(b'\n}\n',start)+3;region=original[start:end]
    assert region.count(before.encode())==1,name
    try:
        SOURCE.write_bytes(original[:start]+region.replace(before.encode(),after.encode())+original[end:])
        code,output=run()
        assert code==101 and 'test result: FAILED' in output and f'---- {TEST} stdout ----' in output,output
        assert 'native websocket field guard/borrow' in output,output
    finally: SOURCE.write_bytes(original)
    assert SOURCE.read_bytes()==original
    code,output=run(); assert code==0 and 'test result: ok. 1 passed' in output,output
    print(f'field {name}: compiled named native failure101; exact restoration PASS',flush=True)
