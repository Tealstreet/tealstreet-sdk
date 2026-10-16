from pathlib import Path
import os
import subprocess
import json
import sys

root=Path(__file__).resolve().parents[3]
venue=root/'sdk-rs/safe-cex-venue-bitunix'
out=root/'sdk-rs/target/bitunix-history-read-evidence'
out.mkdir(parents=True,exist_ok=True)
env=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
controls=[
 ('pager-skip','history_reads.rs','fetch_bitunix_history_pages','page as f64 * 100.0','page as f64 * 99.0','history_reads','actual_typescript_history_pager','HREAD native effect get'),
 ('position-selectors','history_reads.rs','fetch_closed_position_rows','selectors(&["mtime", "ctime"], "positionId")','selectors(&["ctime", "mtime"], "positionId")','history_reads','actual_typescript_closed_rows','HREAD native effect selectorTime'),
 ('income-endpoint','history_reads.rs','fetch_income_history','endpoint("HISTORY_TRADES")','endpoint("HISTORY_POSITIONS")','history_reads','actual_typescript_income_history','HREAD native effect delegate'),
 ('pnl-filter','history_reads.rs','fetch_closed_pnl','records.push(record);','records.extend(Some(record).into_iter().take(0));','history_reads','actual_typescript_closed_pnl','HREAD native result'),
 ('position-currency','history_reads.rs','fetch_historical_positions','"USDT".encode_utf16()','"USD".encode_utf16()','history_reads','actual_typescript_historical_positions','HREAD native effect resolved'),
 ('order-overlap','history_reads.rs','fetch_historical_orders','end = Some(oldest);','end = Some(oldest - 1.0);','history_reads','actual_typescript_historical_orders','HREAD native effect get'),
 ('ohlcv-limit','history_rest.rs','fetch_ohlcv','requested.min(200.0)','requested.min(199.0)','history_rest','actual_typescript_history_rest','RREST native effect subtract'),
 ('metadata-mode','history_rest.rs','update_symbol_metadata','Some("ISOLATION")','Some("isolation")','history_rest','actual_typescript_history_rest','RREST native effect set'),
 ('validation-code','history_rest.rs','validate_account','!= Some(0.0)','== Some(0.0)','history_rest','actual_typescript_history_rest','RREST native result'),
]
selected=set(sys.argv[1:]);assert not selected-{c[0] for c in controls}
results=[]
for name,file,function,before,after,target,test,assertion in controls:
 if selected and name not in selected:continue
 source=venue/'src'/file;original=source.read_text();start=original.index('pub async fn '+function);end=original.find('\npub ',start+1);end=len(original) if end==-1 else end
 block=original[start:end];assert block.count(before)==1,(name,before)
 command=['cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-bitunix','--jobs','2','--test',target,test,'--','--exact','--nocapture']
 try:
  source.write_text(original[:start]+block.replace(before,after)+original[end:])
  failed=subprocess.run(command,cwd=root,env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
  (out/(name+'-mutated.log')).write_text(failed.stdout)
  assert failed.returncode==101 and 'Finished `test`' in failed.stdout and assertion in failed.stdout,(name,failed.stdout[-2200:])
 finally:source.write_text(original)
 passed=subprocess.run(command,cwd=root,env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
 (out/(name+'-restored.log')).write_text(passed.stdout)
 assert passed.returncode==0,(name,passed.stdout[-2200:])
 results.append({'family':name,'compiled_exit':failed.returncode,'assertion':assertion,'restored_exit':passed.returncode})
 (out/('selected-controls.json' if selected else 'controls.json')).write_text(json.dumps(results,indent=2)+'\n')
 print(name+': compiled native failure 101; restored pass 0',flush=True)
