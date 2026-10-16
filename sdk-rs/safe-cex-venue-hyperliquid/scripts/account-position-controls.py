#!/usr/bin/env python3
"""One grouped production mutation for ten new account/position leaves."""
import argparse, json, re, subprocess, tempfile
from pathlib import Path
CRATE = Path(__file__).resolve().parents[1]
ROOT = CRATE.parents[1]
NAMES = ['core.shouldUseSpotCollateralBalance', 'core.shouldRequestPortfolioMarginSpotState', 'core.shouldTrackSpotHoldings', 'core._getSymbolConfigurationFromAssetData', 'core.getFakePositions', 'core.invalidateFakePositionsCache', 'core._hasFullSizedPositionOppositeSide', 'core.resolveReduceOnly', 'spot._getSymbolConfigurationFromAssetData', 'spot.getFakePositions']
CARGO = ['/opt/homebrew/opt/rustup/bin/cargo', 'test', '--manifest-path', 'sdk-rs/Cargo.toml', '-p', 'safe-cex-venue-hyperliquid', '--test', 'account_position_leaves', '--jobs', '2']
def replace(s, old, new, count=1):
    assert s.count(old) == count, (old, s.count(old))
    return s.replace(old, new)
if __name__ == '__main__':
    p = argparse.ArgumentParser()
    p.add_argument('--output', required=True, type=Path)
    args = p.parse_args()
    sha = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
    subprocess.run(['git', 'diff', '--exit-code', 'HEAD', '--', str(CRATE/'src'), str(CRATE/'tests'), str(CRATE/'scripts')], cwd=ROOT, check=True)
    source = CRATE/'src/account_position_leaves.rs'
    original = source.read_bytes()
    logs = Path(tempfile.mkdtemp(prefix='hyperliquid-account-position-controls-'))
    def run(label):
        r = subprocess.run(CARGO, cwd=ROOT, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        (logs/(label+'.log')).write_text(r.stdout)
        print(label+':exit'+str(r.returncode), flush=True)
        return r
    try:
        s = original.decode()
        start = s.index('pub fn should_use_spot_collateral_balance')
        end = s.index('pub fn should_request_portfolio_margin_spot_state')
        section = replace(s[start:end], 'return Ok(true);', 'return Ok(false);')
        s = s[:start]+section+s[end:]
        start = s.index('pub fn should_request_portfolio_margin_spot_state')
        end = s.index('pub fn should_track_spot_holdings')
        section = replace(s[start:end], 'let portfolio = literal(h, "portfolioMargin");', 'let portfolio = literal(h, "CONTROL_PORTFOLIO");')
        s = s[:start]+section+s[end:]
        s = replace(s, 'method(h, exchange, "shouldUseSpotCollateralBalance", &[])?', 'method(h, exchange, "CONTROL_COLLATERAL", &[])?')
        s = replace(s, 'h.number(20.0)', 'h.number(21.0)')
        s = replace(s, 'let class = h.global("HyperliquidSpotExchange")?;', 'let class = h.global("HyperliquidExchange")?;')
        s = replace(s, 'let generate = h.get(exchange, "generateFakePositionsFromMarkets")?;', 'let generate = h.get(exchange, "CONTROL_GENERATOR")?;', 2)
        s = replace(s, 'let null = h.null();', 'let null = h.undefined();')
        s = replace(s, 'h.boolean(false)', 'h.boolean(true)')
        s = replace(s, 'h.greater_equal(&contracts, &capture.amount)', 'h.greater_equal(&capture.amount, &contracts)')
        source.write_text(s)
        mutation = run('grouped-ten-mutation')
    finally:
        source.write_bytes(original)
    restored = run('restored')
    observed = [n for n in NAMES if re.search(r'failed: POSITION native[^\n]*(?<![A-Za-z0-9_])'+re.escape(n)+r'\b', mutation.stdout)]
    report = {'measured_sha':sha, 'cargo_command':CARGO, 'mutation_exit':mutation.returncode, 'required_named_native_assertions':NAMES, 'observed_named_native_assertions':observed, 'mutation_output':mutation.stdout, 'restored_exit':restored.returncode, 'restored_output':restored.stdout, 'bytes_restored':source.read_bytes()==original, 'logs':str(logs), 'opposite_pair':{'contracts':91.25,'amount':17.5,'control':'swap operands in owned Rust callback >=; native arguments asserted before aggregate'}, 'iteration_stop':'ten roots in one grouped production control, source test129cases through128; virtual generators/fullclasses/intrinsic collections/global primitives outside proof'}
    args.output.write_text(json.dumps(report, indent=2)+'\n')
    assert mutation.returncode == 101 and 'test result: FAILED. 0 passed; 1 failed' in mutation.stdout and 'error[E' not in mutation.stdout, mutation.stdout
    assert observed == NAMES, 'survivors:'+repr(set(NAMES)-set(observed))
    assert restored.returncode == 0 and '1 passed; 0 failed' in restored.stdout, restored.stdout
    assert report['bytes_restored']
    print('all ten named native assertions discriminate; restored test passed', flush=True)
