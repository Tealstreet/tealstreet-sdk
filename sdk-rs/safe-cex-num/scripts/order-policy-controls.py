"""Require compiled named failures for each portable order arithmetic branch."""
import os
from pathlib import Path
import subprocess

root = Path(__file__).resolve().parents[3]
source = root / 'sdk-rs/safe-cex-num/src/order.rs'
base = ['cargo', 'test', '--manifest-path', str(root / 'sdk-rs/Cargo.toml'), '-p', 'safe-cex-num', '--test', 'order_policy', '--jobs', '2']
controls = [
    ('reduce-only-fallback', 'rounded > 0.0 || !reduce_only', 'rounded > 0.0 || reduce_only', 'ORDER native roundMarketOrderSize bits'),
    ('breakeven-side', 'side == PositionSide::Long', 'side == PositionSide::Short', 'ORDER native breakevenPrice bits'),
    ('inverse-notional', 'market.is_inverse == Some(true)', 'market.is_inverse == Some(false)', 'ORDER native notionalToNativeSize'),
    ('spot-venue', 'Some("hyperliquid" | "hyperliquidspot")', 'Some("bybit" | "hyperliquidspot")', 'ORDER native shouldRoundSizeDown'),
]
for name, before, after, assertion in controls:
    original = source.read_bytes()
    text = original.decode()
    if text.count(before) != 1:
        raise SystemExit(f'{name}: mutation site drift')
    try:
        source.write_text(text.replace(before, after))
        result = subprocess.run(base, cwd=root, capture_output=True, text=True)
        output = result.stdout + result.stderr
        if result.returncode != 101 or assertion not in output or 'error[E' in output:
            raise SystemExit(f'{name}: no compiled named failure\n{output}')
        print(f'{name}: exit 101 at {assertion}', flush=True)
    finally:
        source.write_bytes(original)
    restored = subprocess.run(base, cwd=root, capture_output=True, text=True)
    if restored.returncode:
        raise SystemExit(restored.stdout + restored.stderr)
    print(f'{name}: restored exit 0', flush=True)

source = root / 'sdk-rs/safe-cex-num/src/order_policy.rs'
base[base.index('order_policy')] = 'order_policy_market'
controls = [
    ('price-identity-swap', 'out.amount = amount;', 'out.amount = amount; std::mem::swap(&mut out.price, &mut out.stop_loss);', 'ORDER market native price bits'),
    ('infinity-stop-gate', 'venue_supports_infinity_stops(name)? != Some(true)', 'venue_supports_infinity_stops(name)? == Some(true)', 'ORDER prepare unexpected refusal'),
]
for name, before, after, assertion in controls:
    original = source.read_bytes()
    text = original.decode()
    if text.count(before) != 1:
        raise SystemExit(f'{name}: mutation site drift')
    try:
        source.write_text(text.replace(before, after))
        result = subprocess.run(base, cwd=root, capture_output=True, text=True)
        output = result.stdout + result.stderr
        if result.returncode != 101 or assertion not in output or 'error[E' in output:
            raise SystemExit(f'{name}: no compiled named failure\n{output}')
        print(f'{name}: exit 101 at {assertion}', flush=True)
    finally:
        source.write_bytes(original)
    restored = subprocess.run(base, cwd=root, capture_output=True, text=True)
    if restored.returncode:
        raise SystemExit(restored.stdout + restored.stderr)
    print(f'{name}: restored exit 0', flush=True)
