#!/usr/bin/env python3
"""One production mutation per utility row; serial, corpus-batched live witnesses."""
from pathlib import Path
import argparse
import os
import subprocess
import tomllib

crate = Path(__file__).resolve().parents[1]
workspace = crate.parent
command = ['/opt/homebrew/opt/rustup/bin/cargo', 'test', '--locked', '--manifest-path', str(workspace / 'Cargo.toml'),
           '-p', 'safe-cex-venue-kraken', '--test', 'reads', '--jobs', '1']
production = [
    ('kraken.exchange.KrakenExchange#_fetchAccountLogPage','src/reads.rs','JsJsonValue::Number(params.start_time)','JsJsonValue::Number(params.limit)','account-log native since/count query'),
    ('krakenspot.exchange.KrakenSpotExchange.fetchWsToken','src/reads.rs','Ok(token.expect("truthy token is present").clone())','Ok(Value::Null)','token native value/error'),
    ('krakenspot.exchange.KrakenSpotExchange.fetchPositions','src/reads.rs','if !market.active {','if market.active {','Spot positions native fields and order'),
    ('kraken.exchange.KrakenExchange.fetchLeverageSettings','src/reads.rs','symbol.expect("truthy symbol").clone(),\n                                        max_leverage.expect("truthy leverage").clone(),','max_leverage.expect("truthy leverage").clone(),\n                                        symbol.expect("truthy symbol").clone(),','leverage native cache mutation trace'),
    ('kraken.exchange.KrakenExchange.fetchHistoricalPositions','src/reads.rs','        false,\n    )\n    .await','        true,\n    )\n    .await','HISTORY native fold arguments'),
]
supplemental = [
    ('symmetric-serde-query-swap','src/reads.rs','    start_time: f64,\n    limit: f64,','    #[serde(rename = "limit")]\n    start_time: f64,\n    #[serde(rename = "startTime")]\n    limit: f64,','account-log native since/count query'),
    ('pre-await-store-snapshot','src/reads.rs','        let data = response?.await?.map(wire::decode).transpose()?;','        let snapshot = store.markets_by_symbol().clone();\n        let data = response?.await?.map(wire::decode).transpose()?;','account-log native records and error'),
]
for field,value in [('entry_price','0.0'),('notional','0.0'),('leverage','1.0'),('unrealized_pnl','0.0'),('contracts','0.0'),('liquidation_price','0.0')]:
    supplemental.append((f'position-{field}','src/reads.rs',f'{field}: {value},',f'{field}: 7.25,','Spot positions native fields and order'))
leverage_controls = [
    ('leverage-clear-omitted','src/reads.rs','                clear()?;','                if false { clear()?; }','leverage native cache mutation trace'),
    ('leverage-success-guard','src/reads.rs','== Some("success")','== Some("failure")','leverage native cache mutation trace'),
    ('leverage-empty','src/reads.rs','for preference in preferences {','for preference in preferences.iter().take(0) {','leverage native cache mutation trace'),
    ('leverage-subset','src/reads.rs','for preference in preferences {','for preference in preferences.iter().take(1) {','leverage native cache mutation trace'),
    ('leverage-extra','src/reads.rs','for preference in preferences {','for preference in preferences.iter().chain(preferences.iter().take(1)) {','leverage native cache mutation trace'),
    ('leverage-truthiness','src/reads.rs','if js_truthy(max_leverage) {','if max_leverage.is_some() {','leverage native cache mutation trace'),
    ('leverage-console-metadata-flattened','src/reads.rs','&error.message,\n            )?;','&error.message,\n            ).map_err(|error| ExchangeError::new(error.message))?;','leverage console failure subscriber metadata'),
    ('leverage-object-key-collapse','tests/reads.rs','!symbol.is_object() && !symbol.is_array() && *key == symbol','*key == symbol','leverage ordered partial cache state'),
]
parser = argparse.ArgumentParser()
parser.add_argument('--supplemental', action='store_true')
parser.add_argument('--leverage-supplemental', action='store_true')
parser.add_argument('--start-at', choices=[row[0] for row in production])
args = parser.parse_args()
expected=set()
for part in ['kraken.exchange','krakenspot.exchange']:
    manifest=workspace / 'parity/manifest/exchanges/kraken' / f'{part}.ts.toml'
    expected.update(part+'.'+row['ts'] for row in tomllib.loads(manifest.read_text())['symbol'] if row['rs'].startswith('safe-cex-venue-kraken/src/reads.rs::'))
assert len(production)==len(expected) and {row[0] for row in production}==expected, 'read production mutation row census'
start=next((i for i,row in enumerate(production) if row[0]==args.start_at),0)
controls=production[start:]+(supplemental if args.supplemental else [])+(leverage_controls if args.leverage_supplemental else [])
def run():
    return subprocess.run(command,cwd=workspace,env=os.environ,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
baseline=run()
assert baseline.returncode==0, baseline.stdout
for name,file,old,new,assertion in controls:
    target=crate/file
    original=target.read_bytes()
    source=original.decode()
    assert source.count(old)==1,(name,'mutation anchor count',source.count(old))
    try:
        mutated=source.replace(old,new)
        if name == 'pre-await-store-snapshot':
            anchor='store\n                .markets_by_symbol()'
            assert mutated.count(anchor)==1
            mutated=mutated.replace(anchor,'snapshot')
        target.write_text(mutated)
        broken=run()
        assert broken.returncode not in (0,126) and assertion in broken.stdout and 'test result: FAILED' in broken.stdout,(name,broken.stdout)
        print(f'{name}: failed named assertion {assertion}',flush=True)
    finally:
        target.write_bytes(original)
    restored=run()
    assert restored.returncode==0,(name,'restored failure',restored.stdout)
    print(f'{name}: restored pass',flush=True)
print(f'{len(controls)} mutations failed named assertions; {len(controls)} restorations passed')
