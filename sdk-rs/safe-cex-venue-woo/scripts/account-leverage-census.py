"""Independent tomllib census: final worktree versus an integration commit."""
from pathlib import Path
from collections import Counter
import tomllib,subprocess,json,sys
parent=sys.argv[1];root=Path('sdk-rs/parity/manifest')
files=sorted((root/'exchanges/woo').glob('*.toml'))+[root/'journal/woo.position-adapter.ts.toml']
assert len(files)==22
expected={'requireEnum', 'WooV3LeverageReadback', 'WOO_V3_ACCOUNT_WRITE_PARITY', 'decodeWooV3GetLeverageResponse', 'requirePerpetualSymbol', 'requireExactKeys', 'WooV3GetLeverageInput', 'WooV3AccountConfigResponseError.constructor', 'WooV3AccountMutationResult', 'decodeSuccess', 'requireBoolean', 'buildWooV3SetLeverageRequest', 'decodeWooV3SetLeverageResponse', 'WooV3AccountConfigResponseError', 'WooV3AccountPositionMode', 'buildWooV3GetLeverageRequest', 'WooV3AccountModeProvenance', 'WooV3SetLeverageBuildResult', 'WooV3SetLeverageInput', 'requireRecord', 'WooV3FuturesMarginMode', 'validateAccountProvenance', 'requireString', 'WooV3GetLeverageRequest', 'requireInteger'}
assert len(expected)==25
ported={"WooV3AccountPositionMode","WooV3FuturesMarginMode"}
before={};after={}
for file in files:
 for output,text in [(before,subprocess.check_output(['git','show',parent+':'+str(file)],text=True)),(after,file.read_text())]:
  for row in tomllib.loads(text)['symbol']:
   key=(str(file),row['ts'],row['kind']);assert key not in output;output[key]=row
assert set(before)==set(after) and len(before)==534
moved=[{'file':key[0],'ts':key[1],'before':before[key]['status'],'after':after[key]['status'],'rs':after[key]['rs']} for key in before if before[key]['status']!=after[key]['status']]
assert len(moved)==25 and {row['ts'] for row in moved}==expected
assert all(row['before']=='pending' and row['after']==('ported' if row['ts'] in ported else 'divergent') for row in moved)
print(json.dumps({'parent':subprocess.check_output(['git','rev-parse',parent],text=True).strip(),'sha':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),'manifest_files':22,'rows':534,'before':dict(Counter(r['status'] for r in before.values())),'after':dict(Counter(r['status'] for r in after.values())),'rows_moved':25,'ported':2,'divergent':23,'moved':moved},indent=2))
