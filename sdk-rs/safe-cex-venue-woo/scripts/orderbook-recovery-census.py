"""Independent tomllib census: final worktree versus an integration commit."""
from pathlib import Path
from collections import Counter
import tomllib,subprocess,json,sys
parent=sys.argv[1];root=Path('sdk-rs/parity/manifest')
files=sorted((root/'exchanges/woo').glob('*.toml'))+[root/'journal/woo.position-adapter.ts.toml']
assert len(files)==22
expected={'WooV3OrderBookRecoveryCoordinator#reset', 'WooV3OrderBookRecoveryCoordinator.get requiresFreshSnapshot', 'WooV3OrderBookRecoveryCoordinator#bufferBeforeSnapshot', 'WooV3OrderBookRecoveryCoordinator#applyToSequencer', 'WooV3OrderBookRecoveryCoordinator#createPublication', 'normalizeUpdate', 'WooV3OrderBookRecoveryCoordinator.reconnect', 'requirePositiveTimestamp', 'WooV3RecoveredOrderBook', 'WooV3OrderBookRecoveryCoordinator#joinAfterSnapshot', 'normalizeSide', 'WooV3OrderBookRecoveryCoordinator', 'WooV3OrderBookRecoveryCoordinator.pushUpdate', 'WooV3OrderBookRecoveryCoordinator.acceptRestSnapshot', 'WooV3OrderBookRecoveryCoordinator#applyJoinedUpdate', 'WOO_V3_ORDERBOOK_MAX_BUFFERED_UPDATES', 'WooV3OrderBookRecoveryCoordinator.constructor', 'WooV3OrderBookRecoveryResult', 'WooV3OrderBookRecoveryCoordinator.beginSnapshotRequest', 'isRecord', 'snapshotLevels', 'WooV3OrderBookRecoveryCoordinator#recover', 'requireNumericLevel'}
assert len(expected)==23
ported={"WOO_V3_ORDERBOOK_MAX_BUFFERED_UPDATES","WooV3OrderBookRecoveryCoordinator.get requiresFreshSnapshot","WooV3OrderBookRecoveryCoordinator#applyToSequencer","WooV3OrderBookRecoveryCoordinator#reset"}
before={};after={}
for file in files:
 for output,text in [(before,subprocess.check_output(['git','show',parent+':'+str(file)],text=True)),(after,file.read_text())]:
  for row in tomllib.loads(text)['symbol']:
   key=(str(file),row['ts'],row['kind']);assert key not in output;output[key]=row
assert set(before)==set(after) and len(before)==534
moved=[{'file':key[0],'ts':key[1],'before':before[key]['status'],'after':after[key]['status'],'rs':after[key]['rs']} for key in before if before[key]['status']!=after[key]['status']]
assert len(moved)==23 and {row['ts'] for row in moved}==expected
assert all(row['before']=='pending' and row['after']==('ported' if row['ts'] in ported else 'divergent') for row in moved)
print(json.dumps({'parent':subprocess.check_output(['git','rev-parse',parent],text=True).strip(),'sha':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),'manifest_files':22,'rows':534,'before':dict(Counter(r['status'] for r in before.values())),'after':dict(Counter(r['status'] for r in after.values())),'rows_moved':23,'ported':4,'divergent':19,'moved':moved},indent=2))
