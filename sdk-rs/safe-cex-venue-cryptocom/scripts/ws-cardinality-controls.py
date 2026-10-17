"""Fully decode each selected collection before corrupting its retained cardinality."""
from pathlib import Path
import subprocess
SDK = Path(__file__).resolve().parents[2]
SOURCE = SDK / 'safe-cex-venue-cryptocom/src/ws_payloads.rs'
SITES = [('TickerUpdate', 'data', 'array(field(value, "data")?, CryptocomWsTickerData::from_value)?', False), ('ValuationUpdate', 'data', 'array(field(value, "data")?, CryptocomWsValuationData::from_value)?', False), ('TradeUpdate', 'data', 'array(field(value, "data")?, CryptocomWsTradeData::from_value)?', False), ('BookUpdate', 'data', 'array(field(value, "data")?, CryptocomWsBookData::from_value)?', False), ('OrderUpdate', 'data', 'array(field(value, "data")?, CryptocomWsOrderData::from_value)?', False), ('PositionUpdate', 'data', 'array(field(value, "data")?, CryptocomWsPositionData::from_value)?', False), ('CandlestickUpdate', 'data', 'array(\n                field(value, "data")?,\n                CryptocomWsCandlestickData::from_value,\n            )?', False), ('BalanceUpdate', 'data', 'array(field(value, "data")?, CryptocomWsBalanceSummary::from_value)?', False), ('BookData', 'bids', 'array(field(value, "bids")?, tuple)?', False), ('BookData', 'asks', 'array(field(value, "asks")?, tuple)?', False), ('OrderData', 'exec_inst', 'optional(field(value, "exec_inst")?, |v| array(v, string))?', True), ('BalanceSummary', 'position_balances', 'array(\n                field(value, "position_balances")?,\n                CryptocomWsPositionBalance::from_value,\n            )?', False)]
def run(test):
    r = subprocess.run(['cargo', 'test', '--locked', '--jobs', '1', '-q', '-p', 'safe-cex-venue-cryptocom', '--test', 'accepted_ws_record_shapes', test, '--', '--exact'], cwd=SDK, capture_output=True, text=True)
    return r.returncode, r.stdout+r.stderr
for owner, field, expression, optional in SITES:
    test = 'ws_record_'+owner.lower()
    code, output = run(test)
    assert code == 0 and 'test result: ok. 1 passed' in output, output
    original = SOURCE.read_bytes()
    start = original.index(('impl CryptocomWs'+owner+' {').encode())
    end = original.index(b'\n}\n', start)+3
    region = original[start:end]
    assert region.count(expression.encode()) == 1, (owner, field)
    for label, operation in [('empty','take(0)'),('subset','skip(1)')]:
        replacement = '('+expression+')'
        replacement += ('.map(|values| values.into_iter().'+operation+'.collect())' if optional else '.into_iter().'+operation+'.collect()')
        try:
            SOURCE.write_bytes(original[:start]+region.replace(expression.encode(),replacement.encode())+original[end:])
            code, output = run(test)
            assert code == 101 and 'test result: FAILED' in output and f'---- {test} stdout ----' in output, output
            assert 'CryptocomWs'+owner+' native fields' in output, output
        finally:
            SOURCE.write_bytes(original)
        assert SOURCE.read_bytes() == original
        code, output = run(test)
        assert code == 0 and 'test result: ok. 1 passed' in output, output
        print(f'{owner}.{field} consumed {label}: compiled named native field failure101; exact restoration PASS',flush=True)
