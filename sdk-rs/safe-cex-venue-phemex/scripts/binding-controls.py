"""Compiling production mutations, named runtime failures, and exact restoration.

Run from any directory with the TypeScript oracle dependency configured.
Each Exchange override and each repaired helper has its own fail/restore check.
"""
from pathlib import Path
import os
import re
import subprocess
import sys

ROOT=Path(__file__).resolve().parents[2]
SOURCE=ROOT/'safe-cex-venue-phemex/src/binding/exchange.rs'
METHODS=[
 ('dispose','"dispose"','"__mutation"','start_populates_snapshots_and_opens_both_sockets'),
 ('start','"start"','"__mutation"','start_populates_snapshots_and_opens_both_sockets'),
 ('get_account','"getAccount"','"__mutation"','get_account_uses_signed_account_positions'),
 ('validate_account','"validateAccount"','"__mutation"','validate_account_returns_balance_and_keeps_store_pure'),
 ('fetch_markets','"fetchMarkets"','"__mutation"','fetch_markets_preserves_distinct_steps_and_filters_products'),
 ('fetch_tickers','"fetchTickers"','"__mutation"','fetch_tickers_maps_source_numbers_without_store_writes'),
 ('fetch_orders','"fetchOrders"','"__mutation"','fetch_orders_returns_the_store_snapshot_without_http'),
 ('fetch_positions','"fetchPositions"','"__mutation"','fetch_positions_supports_symbol_recovery_and_pure_full_snapshots'),
 ('place_order','"placeOrder"','"__mutation"','place_order_honors_id_and_distinct_amount_price_precision'),
 ('place_orders','"placeOrders"','"__mutation"','place_orders_preflights_and_places_each_caller_id_in_series'),
 ('update_order','"updateOrder"','"__mutation"','update_order_cancels_then_replaces'),
 ('cancel_orders','"cancelOrders"','"__mutation"','cancel_orders_deduplicates_parent_and_updates_status'),
 ('set_leverage','"setLeverage"','"__mutation"','set_leverage_retains_cross_margin_and_records_confirmed_metadata'),
 ('change_margin_type','"changeMarginType"','"__mutation"','change_margin_type_retains_existing_leverage'),
 ('change_position_mode','"changePositionMode"','"__mutation"','change_position_mode_sends_symbol_and_records_raw_mode'),
 ('fetch_symbol_max_leverage','not_implemented()','Box::pin(async { Ok(0.0) })','inherited_symbol_max_leverage_rejects'),
 ('schedule_cancel','not_implemented()','Box::pin(async { Ok(()) })','inherited_schedule_cancel_rejects'),
 ('fetch_public_trades','not_implemented()','Box::pin(async { Ok(vec![]) })','inherited_public_trades_rejects'),
 ('fetch_historical_fills','"fetchHistoricalFills"','"__mutation"','historical_fills_use_the_primary_symbol_scoped_route'),
]
HELPERS=[
 ('dispose_super','binding/session.rs','self.rt.base().dispose().map_err(Thrown::from)','Ok(())','exchange_binding','dispose_runs_source_super_cleanup'),
 ('symbol_in','utilities.rs','result.push(error.unpaired_surrogate())','result.push({ let _ = error; 0xfffd })','binding_values','symbol_in_preserves_every_lone_surrogate_unit'),
 ('symbol_out','utilities.rs','return Ok(found);','return Ok(Js::str("mutant"));','binding_values','symbol_out_preserves_raw_info_units'),
 ('raw_tickers','ticker_reads.rs','    Ok(data)\n','    Ok(Js::Null)\n','binding_values','raw_tickers_preserve_response_identity_and_units'),
 ('eager_reconciliation','private_session.rs','let started = begin_sync(host);\n    async move { finish_sync(host, started?).await }','async move { let started = begin_sync(host); finish_sync(host, started?).await }','private_transports','reconciliation_starts_account_read_before_the_future_is_polled'),
]
def run(target,test):
    p=subprocess.run(['cargo','test','--locked','-q','--jobs','1','-p','safe-cex-venue-phemex','--test',target,test,'--','--exact'],cwd=ROOT,capture_output=True,text=True)
    return p.returncode,p.stdout+p.stderr

def method(original,name,old,new):
    match=re.search(r'    fn '+name+r'[(<]',original)
    assert match,name
    start=match.start();end=original.find('\n    fn ',start+1);end=len(original) if end<0 else end
    body=original[start:end];assert old in body,(name,old)
    return original[:start]+body.replace(old,new,1)+original[end:]

def main():
    selected=set(sys.argv[1:]);known={r[0] for r in METHODS+HELPERS}
    assert not selected-known,selected-known
    controls=[]
    for name,old,new,test in METHODS:
        if not selected or name in selected: controls.append((name,SOURCE,lambda source,n=name,o=old,v=new:method(source,n,o,v),'exchange_binding',test))
    for name,file,old,new,target,test in HELPERS:
        if not selected or name in selected:
            def change(source,old=old,new=new):
                assert source.count(old)==1,(old,source.count(old))
                return source.replace(old,new,1)
            controls.append((name,ROOT/'safe-cex-venue-phemex/src'/file,change,target,test))
    assert controls,'no selected controls'
    for _,_,_,target,test in controls:
        code,out=run(target,test);assert code==0 and 'test result: ok. 1 passed' in out,(target,test,out)
    for name,path,change,target,test in controls:
        original=path.read_bytes()
        try:
            path.write_text(change(original.decode()))
            code,out=run(target,test)
            assert code==101 and test in out and 'FAILED' in out and 'test result: FAILED' in out,(name,'must fail at runtime',out)
        finally:
            path.write_bytes(original)
        code,out=run(target,test);assert code==0 and 'test result: ok. 1 passed' in out,(name,'restoration',out)
        print(f'{name}: mutant exit101 runtime failure; restored exit0 pass',flush=True)

if __name__=='__main__': main()
