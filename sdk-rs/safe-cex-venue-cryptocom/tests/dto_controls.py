#!/usr/bin/env python3
"""Sequential compiled DTO controls, exact source restoration and SHA-bound receipts."""
from pathlib import Path
import difflib
import hashlib
import json
import os
import re
import signal
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[3]
SDK = ROOT / 'sdk-rs'
WIRE = SDK / 'safe-cex-venue-cryptocom/src/wire.rs'
RECORD = SDK / 'safe-cex-venue-cryptocom/src/wire_record.rs'
BASE = ['cargo', 'test', '--locked', '-p', 'safe-cex-venue-cryptocom', '--jobs', '2']
# Independent named floor: eleven source rows and three nested capture contexts.
CLEARS = [
    ('instrument-extra', 'CryptocomInstrument', 'instrument', 'CryptocomInstrument'),
    ('book-extra', 'CryptocomBook', 'book', 'CryptocomBook'),
    ('book-data-extra', 'CryptocomBookData', 'book', 'CryptocomBook:child'),
    ('ticker-extra', 'CryptocomTicker', 'ticker', 'CryptocomTicker'),
    ('trade-extra', 'CryptocomTrade', 'trade', 'CryptocomTrade'),
    ('candlestick-extra', 'CryptocomCandlestick', 'candlestick', 'CryptocomCandlestick'),
    ('position-balance-extra', 'CryptocomPositionBalance', 'position_balance', 'CryptocomPositionBalance'),
    ('balance-summary-extra', 'CryptocomBalanceSummary', 'balance_summary', 'CryptocomBalanceSummary'),
    ('summary-position-extra', 'CryptocomPositionBalance', 'balance_summary', 'CryptocomBalanceSummary:child'),
    ('order-extra', 'CryptocomOrder', 'order', 'CryptocomOrder'),
    ('accounts-response-extra', 'CryptocomAccountsResponse', 'accounts_response', 'CryptocomAccountsResponse'),
    ('account-identity-extra', 'CryptocomAccountIdentity', 'accounts_response', 'CryptocomAccountsResponse:child'),
    ('trade-fill-extra', 'CryptocomTradeFill', 'trade_fill', 'CryptocomTradeFill'),
    ('error-extra', 'CryptocomError', 'error', 'CryptocomError'),
]

def region(source, start, change):
    begin = source.index(start)
    end = source.index('\n}', begin) + 2
    return source[:begin] + change(source[begin:end]) + source[end:]


def replacing(before, after):
    def change(text):
        assert text.count(before) == 1, (before, text.count(before))
        return text.replace(before, after)
    return change


def decode_change(record, change):
    return lambda text: region(text, "impl<'de> Deserialize<'de> for " + record + ' {', change)


def serializer_change(record, change):
    return lambda text: region(text, 'impl Serialize for ' + record + ' {', change)


def command(test, target='dto_retention'):
    return BASE + ['--test', target, test, '--', '--exact', '--nocapture']


def main():
    assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH'), 'compiler path required'
    sha = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
    assert subprocess.run(['git', 'diff', '--quiet', 'HEAD', '--'], cwd=ROOT).returncode == 0
    selected = set(sys.argv[1:])
    out = SDK / 'target/cryptocom-dto' / sha[:10] / 'controls'
    out.mkdir(parents=True, exist_ok=True)
    assert not (out / 'gates.json').exists(), 'refusing to overwrite measured receipts'
    records = []
    controls = []

    def run(name, cmd):
        assert subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip() == sha
        log = out / (name + '.log')
        print('RUN', name, sha, flush=True)
        started = time.monotonic()
        process = subprocess.Popen(cmd, cwd=SDK, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, start_new_session=True)
        try:
            with log.open('w') as stream:
                for line in process.stdout:
                    line = line.replace(str(ROOT), '<worktree>')
                    stream.write(re.sub(r'/(?:Users|home)/[^\s\)\"\']+', '<external-path>', line))
                    stream.flush()
            code = process.wait()
        except BaseException:
            try:
                os.killpg(process.pid, signal.SIGINT)
                process.wait(timeout=15)
            except (ProcessLookupError, subprocess.TimeoutExpired):
                try:
                    os.killpg(process.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
                process.wait()
            raise
        row = {'sha': sha, 'name': name, 'command': cmd, 'cwd': 'sdk-rs', 'exit': code,
               'seconds': round(time.monotonic()-started, 2), 'log': log.name,
               'log_sha256': hashlib.sha256(log.read_bytes()).hexdigest()}
        records.append(row)
        (out / 'gates.json').write_text(json.dumps(records, indent=2) + '\n')
        print(name, 'EXIT', code, flush=True)
        return code, log.read_text()

    entries = []
    for label, record, test, assertion_label in CLEARS:
        entries.append((label, WIRE, decode_change(record, replacing(
            'record.additional_fields = fields.additional;',
            'record.additional_fields = serde_json::Value::Object(Default::default());')),
            'dto_' + test + '_retains_unknown_fields', 'DTO-NATIVE-COUNT:' + assertion_label, 'decoded-native', [], False))
    for label, key, value, guard in [
        ('overflow-to-null', '__overflow', 'serde_json::Value::Null', 'numeric ingress lost Number at $.__overflow'),
        ('negative-zero-to-positive', '__negativeZero', 'serde_json::json!(0.0)', 'numeric ingress changed Number bits at $.__negativeZero'),
    ]:
        before = 'record.additional_fields = fields.additional;'
        entries.append((label, WIRE, decode_change('CryptocomInstrument', replacing(before, before + '\n                record.additional_fields["' + key + '"] = ' + value + ';')),
                        'dto_instrument_retains_unknown_fields', 'DTO-ADMISSION:CryptocomInstrument:raw', 'native-admission-guard', [guard], False))
    entries += [
        ('index-order', RECORD, replacing('.collect::<JsObjectMap<_>>()\n                .into_iter()\n                .collect(),', '.collect::<Vec<_>>(),'),
         'dto_instrument_retains_unknown_fields', 'DTO-NATIVE-KEYS:CryptocomInstrument', 'decoded-native', [], False),
        ('known-extra-interleaving', RECORD, replacing('for key in captured {', 'for key in captured.iter().take(0) {'),
         'dto_instrument_retains_unknown_fields', 'DTO-NATIVE-ORDER:CryptocomInstrument', 'decoded-native', [], False),
        ('stale-known-field', WIRE, serializer_change('CryptocomInstrument', replacing('"symbol" => output.serialize_entry(&key, &self.symbol)?,', '"symbol" => output.serialize_entry(&key, &"captured-stale")?,')),
         'dto_instrument_retains_unknown_fields', 'DTO-WIRE-CURRENT-KNOWN:CryptocomInstrument', 'wire-after-native', [], False),
    ]
    for label, record, test, assertion_label, legacy in [
        ('book-child-wire-omission', 'CryptocomBookData', 'book', 'CryptocomBook:child', True),
        ('account-child-wire-omission', 'CryptocomAccountIdentity', 'accounts_response', 'CryptocomAccountsResponse:child', False),
        ('summary-child-wire-omission', 'CryptocomPositionBalance', 'balance_summary', 'CryptocomBalanceSummary:child', False),
    ]:
        entries.append((label, WIRE, serializer_change(record, replacing('for key in keys {', 'for key in keys {\n            if key == "__overflow" { continue; }')),
                        'dto_' + test + '_retains_unknown_fields', 'DTO-WIRE-ORDER:' + assertion_label, 'wire-after-native', [], legacy))
    assert len(entries) == 22 and len({entry[0] for entry in entries}) == 22
    assert not selected - {entry[0] for entry in entries}, 'unknown DTO control names'
    chosen = [entry for entry in entries if not selected or entry[0] in selected]
    baseline, output = run('baseline', BASE + ['--test', 'dto_retention', '--', '--test-threads=1', '--nocapture'])
    assert baseline == 0 and '11 passed; 0 failed;' in output
    for label, file, mutation, test, marker, layer, extra_markers, legacy in chosen:
        original = file.read_bytes()
        text = original.decode()
        changed = mutation(text)
        assert text != changed
        patch = out / (label + '.patch')
        patch.write_text(''.join(difflib.unified_diff(text.splitlines(True), changed.splitlines(True), fromfile=str(file.relative_to(ROOT)), tofile=str(file.relative_to(ROOT)))))
        (out / (label + '.original')).write_bytes(original)
        survivor = None
        try:
            file.write_text(changed)
            if legacy:
                legacy_exit, legacy_output = run(label + '-legacy-witness', command('live_wire_records', 'types'))
                assert legacy_exit == 0 and '1 passed; 0 failed;' in legacy_output
                survivor = {'callsite': 'wire.rs::CryptocomBookData::serialize', 'legacy_witness': 'types.rs::live_wire_records', 'exit': legacy_exit,
                            'type': 'behaviour witness coverage gap', 'reason': 'legacy corpus lacks this overflow extension; new independent nested wire assertions catch omission'}
            broken, output = run(label + '-mutated', command(test))
        finally:
            file.write_bytes(original)
        restored, restored_output = run(label + '-restored', command(test))
        valid = (broken == 101 and 'could not compile' not in output and 'Finished `test`' in output
                 and 'running 1 test' in output and 'panicked at' in output and marker in output
                 and 'test result: FAILED. 0 passed; 1 failed;' in output and all(value in output for value in extra_markers))
        clean = restored == 0 and 'test ' + test + ' ... ok' in restored_output and '1 passed; 0 failed;' in restored_output
        row = {'sha': sha, 'name': label, 'test': test, 'marker': marker, 'additional_markers': extra_markers, 'assertion_layer': layer,
               'mutated': broken, 'restored': restored, 'compiled_discrimination': valid, 'restored_single_test_pass': clean,
               'exact_source_restored': file.read_bytes() == original, 'patch': patch.name, 'patch_sha256': hashlib.sha256(patch.read_bytes()).hexdigest(),
               'floor': 'one mutation per named call site, stopped iteration0'}
        if survivor:
            row['survivor_finding'] = survivor
        controls.append(row)
        (out / 'controls.json').write_text(json.dumps(controls, indent=2) + '\n')
        assert valid and clean and row['exact_source_restored'], row
    assert subprocess.run(['git', 'diff', '--quiet', 'HEAD', '--'], cwd=ROOT).returncode == 0
    assert len(controls) == len(chosen)
    print('COMPLETE', len(controls), 'compiling named101-to0 pairs; exact sources restored', flush=True)


if __name__ == '__main__':
    main()
