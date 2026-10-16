"""One production mutation per OkxExchange trait dispatch, each failing its named E2E test."""
import pathlib
import subprocess

root = pathlib.Path(__file__).resolve().parents[3]
source = root / 'sdk-rs/safe-cex-venue-okx/src/binding/exchange.rs'
original = source.read_text()
log_dir = root / 'sdk-rs/target/okx-exchange-binding-controls'
log_dir.mkdir(parents=True, exist_ok=True)
base = ['cargo', 'test', '--manifest-path', 'sdk-rs/Cargo.toml', '-p', 'safe-cex-venue-okx',
        '--jobs', '2', '--test', 'exchange_binding', '--']

dispatch = [
    ('self.invoke("fetchMarkets", vec![])', 'fetch_markets_maps_instruments'),
    ('self.invoke("fetchTickers", vec![])', 'fetch_tickers_parses_against_loaded_markets'),
    ('self.invoke("validateAccount", vec![])', 'validate_account_signs_the_balance_read'),
    ('self.invoke("fetchPositions", a)', 'fetch_positions_maps_net_positions'),
    ('self.invoke("fetchOrders", a)', 'fetch_orders_reads_normal_and_algo_pages'),
    ('self.invoke("placeOrder", a)', 'place_order_batches_a_broker_tagged_limit'),
    ('self.invoke("placeOrders", a)', 'place_orders_sends_one_batch'),
    ('self.invoke_unit("setLeverage", a)', 'set_leverage_posts_cross_leverage'),
    ('self.invoke_unit("changePositionMode", a)', 'change_position_mode_posts_long_short_mode'),
    ('self.invoke_unit("changeMarginType", a)', 'change_margin_type_reports_cross_only_without_a_request'),
    ('self.invoke("updateOrder", a)', 'update_and_cancel_orders_hit_amend_and_cancel_batch'),
    ('self.invoke_unit("cancelOrders", a)', 'update_and_cancel_orders_hit_amend_and_cancel_batch'),
    ('self.invoke("fetchHistoricalFills", a)', 'fetch_historical_fills_maps_executions'),
    ('self.invoke_unit("start", vec![])', 'start_loads_markets_tickers_account_and_sockets'),
    ('self.call("dispose", vec![])', 'start_loads_markets_tickers_account_and_sockets'),
]
mutations = [(site, site.replace('("', '("missing', 1), test) for site, test in dispatch]
rejection = 'Err(ExchangeError::new("Not implemented"))'
for nth in range(4):
    mutations.append((rejection, nth, 'inherited_base_methods_reject_without_a_request'))


def mutate(site, replacement):
    if isinstance(replacement, int):
        parts = original.split(site)
        if len(parts) != 5:
            raise RuntimeError('expected four inherited rejections')
        out = parts[0]
        for i, part in enumerate(parts[1:]):
            out += (site.replace('Not implemented', 'Not Implemented') if i == replacement else site) + part
        return out
    if original.count(site) != 1:
        raise RuntimeError(f'expected exactly one production site: {site}')
    return original.replace(site, replacement)


try:
    for index, (site, replacement, test) in enumerate(mutations):
        source.write_text(mutate(site, replacement))
        run = subprocess.run(base + [test, '--exact'], cwd=root, text=True,
                             stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        (log_dir / f'{index:02}.log').write_text(run.stdout)
        if run.returncode != 101 or f'test {test} ... FAILED' not in run.stdout:
            raise RuntimeError(f'mutation {index} ({site}) did not fail {test}: {run.returncode}')
        print(f'{index:02} {test}: exit 101', flush=True)
finally:
    source.write_text(original)
restored = subprocess.run(base, cwd=root, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
(log_dir / 'restored.log').write_text(restored.stdout)
if restored.returncode != 0 or '14 passed; 0 failed' not in restored.stdout:
    raise RuntimeError(f'restored target failed: {restored.returncode}\n{restored.stdout}')
print(f'{len(mutations)} controls failed by name; restored 14/14', flush=True)
