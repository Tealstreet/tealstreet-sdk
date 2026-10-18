"""Production mutations for the four read-format rows, one corpus per run."""
import os
import argparse
from pathlib import Path
import subprocess

crate=Path(__file__).resolve().parents[1]
source=crate/'src/portfolio.rs'
command=['cargo','test','-p','safe-cex-venue-breakoutprop','--test','portfolio','--jobs','1','--','--nocapture']
env=dict(os.environ)
env.setdefault('SAFE_CEX_TYPESCRIPT_PATH', subprocess.check_output(['node','-p',"require.resolve('typescript')"],cwd=crate.parent,text=True).strip())
mutations=[
 ('formatBalance','free: total - used,','free: total + used,','portfolio_formatBalance'),
 ('formatOpenPositions','let contracts = position.get("quantity")?;','let contracts = position.get("quantityNotional")?;','portfolio_formatOpenPositions'),
 ('formatPositions','side: (PositionSide::Long).into(),','side: (PositionSide::Short).into(),','portfolio_formatPositions'),
 ('formatOrders','let filled = leg.get("filledQuantity")?;','let filled = leg.get("remainingQuantity")?;','portfolio_formatOrders'),
 ('unknown-enum-refusal','                    safe_cex_num::TableHit::Missing => Js::Undefined,','                    safe_cex_num::TableHit::Missing => { refusal.get_or_insert_with(|| format!("Unrepresentable order {field}: {key}")); Js::Undefined },','portfolio_formatOrders'),
 ('inherited-deferred-refusal','            let date = ops::to_primitive(&order.get("transactionTime")?)?;','            if let Some(error) = refusal.as_ref() { return Err(ExchangeError::new(error.clone())); }\n            let date = ops::to_primitive(&order.get("transactionTime")?)?;','portfolio_formatOrders'),
 ('retained-info','js_deserialize_passthrough(d)','<Value as serde::Deserialize>::deserialize(d)','portfolio_formatBalance'),
 ('native-NaN-fallback','number != 0.0 && !number.is_nan()','number != 0.0','portfolio_formatBalance'),
]
parser=argparse.ArgumentParser()
parser.add_argument("--only", choices=[m[0] for m in mutations])
parser.add_argument("--check-anchors", action="store_true")
args=parser.parse_args()
for name,old,new,assertion in mutations:
    if args.only and name != args.only:continue
    original=source.read_bytes()
    text=original.decode()
    assert text.count(old)==1,(name,text.count(old))
    if args.check_anchors:
        print(f'{name}: one production anchor')
        continue
    try:
        mutated=text.replace(old,new)
        if name=="retained-info":mutated=mutated.replace(", js_deserialize_passthrough", "")
        source.write_text(mutated)
        result=subprocess.run(command,cwd=crate.parent,env=env,capture_output=True,text=True)
        output=result.stdout+result.stderr
        if result.returncode==126:raise RuntimeError('UNKNOWN: exit 126')
        assert result.returncode==101 and assertion in output and 'test result: FAILED' in output,(name,output)
        print(f'{name}: FAIL exit=101 ({assertion})',flush=True)
    finally:
        source.write_bytes(original)
    result=subprocess.run(command,cwd=crate.parent,env=env,capture_output=True,text=True)
    assert result.returncode==0,(name,result.stdout+result.stderr)
    print(f'{name}: restored PASS',flush=True)
