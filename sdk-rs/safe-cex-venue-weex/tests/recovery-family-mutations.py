"""One discriminating production control per recovered row family, run serially.

Also exercise recovered census admission and integration refusal identity after
merge-down. Every control restores its source and proves the affected test passes.
"""
from pathlib import Path
import os
import subprocess

crate = Path(__file__).resolve().parent.parent
files = [crate/'src/types.rs', crate/'src/response_types.rs', crate/'src/utils.rs',
         crate/'tests/candle-oracle.cjs']
originals = {p:p.read_bytes() for p in files}
command = [os.environ.get('CARGO','cargo'),'test','--manifest-path',str(crate.parent/'Cargo.toml'),
           '-p','safe-cex-venue-weex','--jobs','2']
controls = []
def replace(file, before, after, target, label, family):
    original = originals[file]
    assert original.count(before) == 1,(family,before)
    controls.append((file,original.replace(before,after),target,label,family))
replace(files[0],b'("BUY", OrderSide::Buy)',b'("BUY", OrderSide::Sell)',
        'types_oracle','constant::ORDER_SIDE::native','constants (26 rows)')
replace(files[1],b'.decode_with(NumberOrStringWire::deserialize)',
        b'.decode_with(NumberOrStringWire::deserialize)\n            .map(|v| match v { Self::Number(n) => Self::Number(n.abs()), other => other })',
        'response_types','native::kline_signed_zero::direct_raw','response codecs (3 rows)')
source=files[2];original=originals[source]
start=original.index(b'pub fn normalize_position_mode(');end=original.index(b'\n}',start)+2
block=original[start:end]
assert block.count(b'*key == normalized') == 1
controls.append((source,original[:start]+block.replace(b'*key == normalized',b'*key == normalized && normalized == "__mutation__"')+original[end:],
                 'utils_oracle','utility::normalizePositionMode::','enum normalizers (7 rows)'))
replace(source,b'if parsed.is_finite() { parsed } else { fallback }',
        b'if parsed.is_finite() { parsed.abs() } else { fallback }',
        'numeric_utils','numeric_utility::toNumber::','numeric helpers (3 rows)')
replace(source,b'numeric / 1_000_000.0',b'numeric / 100_000.0',
        'timestamps','timestamp::normalizeTimestampToMs::nanoseconds::UTC::route_0','timestamps (2 rows)')
assert original.count(b'price: parsed_price,') == 1 and original.count(b'amount: parsed_amount,') == 1
controls.append((source,original.replace(b'price: parsed_price,',b'price: parsed_amount,').replace(b'amount: parsed_amount,',b'amount: parsed_price,'),
                 'orderbook','book::normalizeOrderBookSide::distinct::route_0::0::price','order books (2 rows)'))
replace(source,b'entry_price: 0.0',b'entry_price: 1.0',
        'fake_positions','fake_position::0::entryPrice','fake positions (1 row)')
replace(source,b'pick(1, &["open", "o"])',b'pick(2, &["open", "o"])',
        'candles','candle::tuple::route_0::open','candles (1 row)')
replace(files[3],b'process.stdout.write(',b'cases.splice(0);process.stdout.write(',
        'candles','cardinality::candles::cases::source_count','recovered census admission')
replace(files[3],b'process.stdout.write(',b'refusals[0]=refusals[1];process.stdout.write(',
        'candles','cardinality::candles::refusals::unique_identities','merged refusal identity')
try:
    for file,mutated,target,label,family in controls:
        file.write_bytes(mutated)
        result=subprocess.run(command+['--test',target],capture_output=True,text=True)
        output=result.stdout+result.stderr
        # A compile failure or an unrelated assertion is never discrimination.
        assert result.returncode == 101 and label in output and 'assertion `left == right` failed' in output,output
        named=next(line.strip() for line in output.splitlines() if 'assertion `left == right` failed' in line)
        print(f'COMPILED exit101 {family}: {named}',flush=True)
        file.write_bytes(originals[file])
        subprocess.run(command+['--test',target],check=True)
        print(f'RESTORED exit0 {target}',flush=True)
finally:
    for file,original in originals.items():file.write_bytes(original)
print('PASS: 8 recovered row families (45 rows) + 2 merged census controls',flush=True)
