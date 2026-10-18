#!/usr/bin/env python3
"""Wrong amount/price precision operands must fail the assembled wire test."""
from pathlib import Path
import subprocess

workspace=Path(__file__).resolve().parents[2]
source=workspace/'safe-cex-venue-apexomni/src/trading.rs'
original=source.read_text()
command=['cargo','test','-p','safe-cex-venue-apexomni','--jobs','1','--test','binding','bound_place_order','--','--exact']
def run():
    return subprocess.run(command,cwd=workspace,text=True,capture_output=True)
try:
    result=run()
    assert result.returncode==0 and '1 passed' in result.stdout,result.stdout+result.stderr
    for before,after,assertion in [
        ('let amount = adjust(opts.amount, p_amount);','let amount = adjust(opts.amount, p_price);','wire size uses amount step'),
        ('let price = present(opts.price).map(|v| adjust(v, p_price));','let price = present(opts.price).map(|v| adjust(v, p_amount));','wire price uses price step'),
    ]:
        assert original.count(before)==1,before
        source.write_text(original.replace(before,after))
        result=run(); output=result.stdout+result.stderr
        assert result.returncode==101 and 'test bound_place_order ... FAILED' in output and assertion in output,output
        print('CONTROL PASS: '+assertion,flush=True)
        source.write_text(original)
finally:
    source.write_text(original)
result=run()
assert result.returncode==0 and '1 passed' in result.stdout,result.stdout+result.stderr
print('RESTORED PASS: distinct precision wire payload',flush=True)
