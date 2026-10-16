"""Fixed 15-row assignment census at explicit commits; not behavioral proof."""
import collections
import json
import subprocess
import sys
import tomllib

selected = [
    ('candleCache/types.ts', 'CandleCacheChunkRecord'),
    ('exchanges/base.ws.ts', 'BaseWebSocket.recordMessageHandlerStats'),
    ('exchanges/base.ws.ts', 'BaseWebSocket#reconcileMessageHandlerStats'),
    ('exchanges/base.ws.ts', 'BaseWebSocket.cleanupMessageHandlerStats'),
    ('instrument-class/bybit.instrumentClass.ts', 'bybitInstrumentClass'),
    ('proxyTokenClient.ts', 'initBroadcastChannel'),
    ('utils/risk.ts', 'calculateOrdersRisk'),
    ('exchanges/okx/okx.exchange.ts', 'OKXExchange.findOrderByClientOrderId'),
    ('exchanges/okx/okx.income.ts', 'okxBillCategory'),
    ('exchanges/okx/okx.income.ts', 'mapOkxBillToIncomeRecords'),
    ('exchanges/bitget/bitget.exchange.ts', 'BitgetExchange#_fetchBillPage'),
    ('exchanges/bitget/bitget.exchange.ts', 'BitgetExchange.mapOrder'),
    ('exchanges/bitget/bitget.income.ts', 'mapBitgetBill'),
    ('exchanges/bitget/bitget.income.ts', 'mapBitgetBills'),
    ('exchanges/bitget/bitgetinverse.exchange.ts', 'BitgetInverseExchange.mapOrder'),
]
assert len(selected) == 15

def census(sha):
    sha = subprocess.check_output(['git', 'rev-parse', sha], text=True).strip()
    rows = []
    for file, symbol in selected:
        path = 'sdk-rs/parity/manifest/' + file + '.toml'
        manifest = tomllib.loads(subprocess.check_output(['git', 'show', sha + ':' + path], text=True))
        found = [row for row in manifest['symbol'] if row['ts'] == symbol]
        assert len(found) == 1, (sha, file, symbol)
        row = found[0]
        rows.append(dict(file=file, ts=symbol, status=row['status'], rs=row['rs'], finding_label='FINDING' in row['note']))
    counts = collections.Counter(row['status'] for row in rows)
    return dict(sha=sha, triple=[counts[key] for key in ['ported', 'divergent', 'pending']], rows=rows)

before, after = map(census, sys.argv[1:3])
transitions = collections.Counter(a['status'] + '->' + b['status'] for a, b in zip(before['rows'], after['rows']))
print(json.dumps(dict(scope='fixed assignment floor; stops BitgetInverseExchange.mapOrder; excludes all other rows', before=before, after=after, transitions=dict(transitions)), indent=2))
