"""Production mutations for the four read-format rows, one corpus per run."""
import os
import argparse
from pathlib import Path
import subprocess

crate=Path(__file__).resolve().parents[1]
source=crate/'src/portfolio.rs'
command=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-breakoutprop','--test','portfolio','--jobs','1','--','--nocapture']
env=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
mutations=[
 ('formatBalance','free: total - used,','free: total + used,','portfolio_formatBalance'),
 ('formatOpenPositions','contracts: position.quantity,','contracts: position.quantity_notional,','portfolio_formatOpenPositions'),
 ('formatPositions','side: PositionSide::Long,','side: PositionSide::Short,','portfolio_formatPositions'),
 ('formatOrders','filled: leg.filled_quantity,','filled: leg.remaining_quantity,','portfolio_formatOrders'),
 ('deferred-enum-refusal','refusal.get_or_insert_with(|| format!("Unrepresentable order status: {other}"));\n                OrderStatus::Open','return Err(ExchangeError::new(format!("Unrepresentable order status: {other}")));','portfolio_formatOrders'),
 ('inherited-deferred-refusal','refusal.get_or_insert_with(|| format!("Unrepresentable order status: {other}"));','if other == "__proto__" { return Err(ExchangeError::new(format!("Unrepresentable order status: {other}"))); }\n                refusal.get_or_insert_with(|| format!("Unrepresentable order status: {other}"));','portfolio_formatOrders'),
 ('retained-info','js_deserialize_passthrough(d)','<Value as serde::Deserialize>::deserialize(d)','portfolio_formatBalance'),
 ('native-NaN-fallback','number != 0.0 && !number.is_nan()','number != 0.0','portfolio_formatBalance'),
]
parser=argparse.ArgumentParser()
parser.add_argument("--only", choices=[m[0] for m in mutations])
args=parser.parse_args()
for name,old,new,assertion in mutations:
    if args.only and name != args.only:continue
    original=source.read_bytes()
    text=original.decode()
    assert text.count(old)==1,(name,text.count(old))
    try:
        mutated=text.replace(old,new)
        if name=="retained-info":mutated=mutated.replace(", js_deserialize_passthrough", "")
        source.write_text(mutated)
        result=subprocess.run(command,env=env,capture_output=True,text=True)
        output=result.stdout+result.stderr
        if result.returncode==126:raise RuntimeError('UNKNOWN: exit 126')
        assert result.returncode==101 and assertion in output and 'test result: FAILED' in output,(name,output)
        print(f'{name}: FAIL exit=101 ({assertion})',flush=True)
    finally:
        source.write_bytes(original)
    result=subprocess.run(command,env=env,capture_output=True,text=True)
    assert result.returncode==0,(name,result.stdout+result.stderr)
    print(f'{name}: restored PASS',flush=True)
