"""Source mutations run sequentially; every compile/test exit is captured and source restored."""
import json,os,pathlib,subprocess,sys
root=pathlib.Path(__file__).resolve().parents[3];source=root/'sdk-rs/safe-cex-venue-coinbase/src/market_position_reads.rs';original=source.read_text();sha=subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip();out=pathlib.Path(sys.argv[1]);out.mkdir(parents=True,exist_ok=True)
corpus='ticker_position_live_native_corpus';controls=[
 ('ticker-fallback-root','Ok(r.host.cached_tickers())','Ok(Rc::new(RefCell::new(r.host.cached_tickers().borrow().clone())))',corpus,'fetchTickersSpot-fallback: cached ticker array identity'),
 ('cfm-ticker-query','spot_cfm(r, Some(json!({"product_type":"FUTURE"})))','spot_cfm(r, Some(json!({"product_type":"SPOT"})))',corpus,'fetchTickersCfm-normal: native value'),
 ('intx-funding-interval','+ interval;','+ interval * 0.0;',corpus,'intx-normal.fundingTime: decoded native number bits'),
 ('v3-percentage','fields.percentage = float(info.get("price_percentage_change_24h"))?;','fields.percentage = 0.0;',corpus,'v3-normal.percentage: decoded native number bits'),
 ('cfm-contract-conversion','multiply(size, contract)','multiply(size, 1.0)',corpus,'cfm-normal.contracts: decoded native number bits'),
 ('intx-unrealized-pnl','(mark - fields.entry_price)','(fields.entry_price - mark)',corpus,'positions-intx-normal.unrealizedPnl: decoded native number bits'),
 ('missing-position-side','side: PositionSide::Long,','side: PositionSide::Short,',corpus,'positions-fill-missing: position side'),
 ('concurrent-dispatch-order','.map(|v| v.clone().expect("settled child"))','.rev().map(|v| v.clone().expect("settled child"))','concurrent_tickers_wait_all_and_keep_input_order','ticker reverse completion preserves input order'),
 ('cfm-live-response-list','let next=reply.items.borrow().get(index).cloned();','let next = if index > 0 {None} else {reply.items.borrow().get(index).cloned()};','cfm_iterates_items_appended_during_market_lookup','CFM live for-of sees appended response item'),
 ('cfm-callback-id','position(p["product_id"].as_str().expect("typed product_id").into())','position(lookup_id.clone())','cfm_lookup_callback_reloads_product_id_and_retains_info','CFM rereads ID after market lookup callback'),
]
env=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript');results=[]
def run(name,test):
 cmd=['cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-coinbase','--test','ticker_position_reads','--jobs','2',test,'--','--exact'];r=subprocess.run(cmd,cwd=root,env=env,capture_output=True,text=True);text=r.stdout+r.stderr;(out/(name+'.log')).write_text(text);return r.returncode,text
try:
 for name,before,after,test,assertion in controls:
  assert before in original,(name,'missing mutation anchor')
  source.write_text(original.replace(before,after,1));broken,text=run(name+'-broken',test);source.write_text(original);restored,clean=run(name+'-restored',test)
  valid=broken==101 and ('test '+test+' ... FAILED')in text and assertion in text and 'error[E'not in text
  results.append(dict(name=name,test=test,assertion=assertion,broken_exit=broken,restored_exit=restored,discriminated=valid,failure_excerpt=[s for s in text.splitlines()if 'failed:'in s or 'FAILED'in s or 'panicked'in s]));(out/'results.json').write_text(json.dumps(dict(sha=sha,controls=results),indent=2)+'\n');print(json.dumps(results[-1]),flush=True)
  if not valid or restored!=0:raise SystemExit(1)
finally:source.write_text(original)
