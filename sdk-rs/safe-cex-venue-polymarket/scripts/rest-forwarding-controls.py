"""Exactly one compiled URL fault at each of the ten newly ported call sites."""
from pathlib import Path
import json, os, subprocess
crate = Path(__file__).resolve().parents[1]
source = crate/'src/rest_forwarding.rs'
logs = Path(os.environ['POLYMARKET_REST_CONTROL_LOG_DIR'])
logs.mkdir(parents=True,exist_ok=True)
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH')
assert not subprocess.check_output(['git','status','--porcelain','--',str(crate),str(crate.parent/'parity/manifest/exchanges/polymarket')],text=True).strip(), 'commit gate inputs first'
controls = [
 ('getSimplifiedMarkets','simplified_markets','endpoint("SIMPLIFIED_MARKETS")','endpoint("SPREAD")'),
 ('getSpread','spread','endpoint("SPREAD")','endpoint("SIMPLIFIED_MARKETS")'),
 ('getGammaMarket','gamma_market','append("/markets/", condition_id)','append("/wrong/", condition_id)'),
 ('getGammaMarketBySlug','gamma_slug','append("/markets/slug/", slug)','append("/wrong/slug/", slug)'),
 ('searchGammaMarkets','gamma_search','"/markets".encode_utf16().collect()','"/wrong".encode_utf16().collect()'),
 ('cancelOrder','single_cancel','endpoint("CANCEL")','endpoint("CANCEL_ALL")'),
 ('cancelOrders','batch_cancel','endpoint("CANCEL_ORDERS")','endpoint("CANCEL_ALL")'),
 ('cancelMarketOrders','market_cancel','endpoint("CANCEL_MARKET_ORDERS")','endpoint("CANCEL_ALL")'),
 ('cancelAllOrders','all_cancel','endpoint("CANCEL_ALL")','endpoint("DELETE_API_KEY")'),
 ('deleteApiKey','delete_key','endpoint("DELETE_API_KEY")','endpoint("CANCEL_ALL")'),
]
assert len(controls)==10 and controls[-1][0]=='deleteApiKey'
original=source.read_text()
summary={'sha':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),'floor':'ten new call sites; stopped after deleteApiKey','controls':[]}
def run(label,test=None):
 command=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-polymarket','--test','rest_forwarding','--locked','--jobs','2','--']
 if test:command.extend(['--exact',test])
 command.append('--nocapture')
 r=subprocess.run(command,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
 (logs/(label+'.log')).write_text(f'exit_code={r.returncode}\n'+r.stdout)
 return r
baseline=run('baseline')
assert baseline.returncode==0 and '11 passed; 0 failed' in baseline.stdout,baseline.stdout
summary['baseline_exit']=baseline.returncode
for name,test,old,new in controls:
 assert original.count(old)==1,name
 try:
  source.write_text(original.replace(old,new))
  broken=run(name+'-mutation',test)
  assertion='native request URL:'+name
  caught=broken.returncode==101 and assertion in broken.stdout and f'test {test} ... FAILED' in broken.stdout
 finally: source.write_text(original)
 restored=run(name+'-restored',test)
 summary['controls'].append({'source_method':name,'test':test,'assertion':assertion,'baseline_exit':baseline.returncode,'mutation_exit':broken.returncode,'restored_exit':restored.returncode,'caught':caught})
 (logs/'summary.json').write_text(json.dumps(summary,indent=2)+'\n')
 assert restored.returncode==0,restored.stdout
 assert caught,f'FINDING survivor or wrong failure {name}\n{broken.stdout}'
 print(f'{name}: baseline0 / mutation101 ({assertion}) / restored0',flush=True)
assert source.read_text()==original
