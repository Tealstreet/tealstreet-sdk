#!/usr/bin/env python3
"""Compile each ticker data fault against the real native constructor witness."""
from pathlib import Path
import os, subprocess, tempfile, json
workspace = Path(__file__).resolve().parents[2]
source = workspace / 'safe-cex-venue-binance/src/public_ticker_events.rs'
original = source.read_bytes()
logs = Path(tempfile.mkdtemp(prefix='binance-public-ticker-controls-'))
cases = [('parse_field', None, None, 'futures')]
for family in ['futures','pm']:
    for channel, good, bad in [('ticker','c','v'),('mark','p','i'),('book','b','a')]:
        cases.append((f'handle_{family}_{channel}_events', good, bad, family))
def run(test):
    p = subprocess.run(['cargo','test','--locked','--jobs','1','-p','safe-cex-venue-binance',
        '--features','native','--test','private_classes',test,'--','--exact','--nocapture'],
        cwd=workspace, env=os.environ.copy(),capture_output=True,text=True,timeout=600)
    return p.returncode,p.stdout+p.stderr
receipts=[]
try:
    for owner, good, bad, family in cases:
        if good is None:
            before = b'crate::private_rest::keyed(event, key)?.float()'
            after = b'{ let _ = key; crate::private_rest::keyed(event, "__missing_ticker_control__")?.float() }'
            assert original.count(before)==1
            changed = original.replace(before,after)
        else:
            start = original.index(f'pub fn {owner}<'.encode())
            end = original.find(b'\n// ts:', start)
            if end<0: end=len(original)
            body = original[start:end]
            before = f'parse_field(&event, "{good}")?'.encode()
            after = f'parse_field(&event, "{bad}")?'.encode()
            assert body.count(before)==1, owner
            changed = original[:start]+body.replace(before,after)+original[end:]
        test=f'production_ticker_coercion_{family}_matches_constructed_typescript'
        source.write_bytes(changed)
        status,output=run(test)
        (logs/f'{owner}-red.log').write_text(output)
        assert status!=0 and f'test {test} ... FAILED' in output and 'could not compile' not in output, output[-9000:]
        source.write_bytes(original)
        status,output=run(test)
        (logs/f'{owner}-restored.log').write_text(output)
        assert status==0 and f'test {test} ... ok' in output and '1 passed; 0 failed' in output, output[-9000:]
        receipts.append({'owner':owner,'test':test,'compiledRed':True,'exactRestoredGreen':True})
        print(f'{owner}: named compiled RED / exact restoration GREEN',flush=True)
finally:
    source.write_bytes(original)
    assert source.read_bytes()==original
(logs/'receipts.json').write_text(json.dumps(receipts,indent=2)+'\n')
print(f'PASS {len(receipts)} ticker body controls; receipts {logs}',flush=True)
