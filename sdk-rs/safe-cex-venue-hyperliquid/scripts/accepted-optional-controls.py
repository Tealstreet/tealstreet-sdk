#!/usr/bin/env python3
"""Remove each reviewed public map guard independently and require native failure."""
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
CRATE = ROOT / 'safe-cex-venue-hyperliquid'
SOURCE = CRATE / 'src/types.rs'
SCHEMA = json.loads((CRATE / 'tests/fixtures/accepted_optional_record_schema.json').read_text())['selected']
assert len(SCHEMA) == 24
ORIGINAL = SOURCE.read_bytes()
ENV = {**os.environ, 'RUSTUP_TOOLCHAIN': '1.99.0', 'RUSTC_BOOTSTRAP': '1', 'CARGO_BUILD_JOBS': '1'}

def run(ident=None, null_field=None, missing_field=None):
    command = [sys.executable, str(CRATE / 'scripts/accepted-nested-native.py'), '--cohort=optional']
    if ident:
        command += ['--owner=' + ident]
        command += ['--missing-only=' + missing_field] if missing_field else ['--null-only=' + null_field] if null_field else ['--array-only']
    result = subprocess.run(command, cwd=ROOT, env=ENV, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    print(result.stdout, flush=True)
    if ident:
        assert result.returncode == 101 and 'could not compile' not in result.stdout and not re.search(r'error\[E\d+\]', result.stdout), 'compiler failure is not a native control'
        assert re.search(r'^test ' + re.escape(ident.lower() + '_native_contract') + r' \.\.\. FAILED$', result.stdout, re.M)
        label = ident + (' required nullable ' + missing_field + ' omission must be rejected' if missing_field else ' present null ' + null_field + ' preserves native presence' if null_field else ' full valid declaration-order array must be rejected')
        assert label in result.stdout
    else:
        assert result.returncode == 0
        for owner in SCHEMA:
            assert re.search(r'^test ' + re.escape(owner.lower() + '_native_contract') + r' \.\.\. ok$', result.stdout, re.M)
try:
    for ident, review in SCHEMA.items():
        assert SOURCE.read_bytes() == ORIGINAL, 'restore the previous owner before mutating another'
        text = ORIGINAL.decode()
        anchor = "impl<'de> Deserialize<'de> for " + ident + ' {'
        assert text.count(anchor) == 1
        start = text.index(anchor)
        end = text.index('\n}', start) + 2
        body = text[start:end]
        assert hashlib.sha256(body.encode()).hexdigest() == review['publicDeserializerSha256'], 'changed owner requires a fresh review'
        assert all(part in body for part in ['struct OnlyMap;', 'JsNumberInput::deserialize(deserializer)?', 'deserialize_map(decoder, OnlyMap)'])
        replacement = anchor + '''
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input.decode_with(|decoder| ''' + ident + '''Wire::deserialize(decoder))
            .map_err(serde::de::Error::custom)
    }
}'''
        print('CONTROL ' + ident + ': remove only the public map guard; retain numeric capture and private wire attributes', flush=True)
        SOURCE.write_text(text[:start] + replacement + text[end:])
        try:
            run(ident)
        finally:
            SOURCE.write_bytes(ORIGINAL)
            assert SOURCE.read_bytes() == ORIGINAL
    # Independently remove representative presence attributes for all three
    # distinct policies: optional non-null, optional nullable and required nullable.
    for ident, rust_field, wire_field, guard_name, missing in [
        ('OrderRequest', 'cloid', 'cloid', 'deserialize_present', False),
        ('SpotToken', 'full_name', 'fullName', 'deserialize_present', False),
        ('SpotAssetCtx', 'mid_px', 'midPx', 'deserialize_required_nullable', True),
    ]:
        assert SOURCE.read_bytes() == ORIGINAL
        text = ORIGINAL.decode()
        start = text.index('struct ' + ident + 'Wire {')
        end = text.index('\n}', start) + 2
        body = text[start:end]
        member = body.index('    pub ' + rust_field + ':')
        attribute = body.rfind('    #[serde(', 0, member)
        assert attribute >= 0
        segment = body[attribute:member]
        guard = 'deserialize_with = "' + guard_name + '"'
        assert segment.count(guard) == 1
        # Both maintained multiline and single-line serde attribute layouts.
        changed_segment = segment.replace('        ' + guard + ',\n', '') if '\n        ' + guard in segment else segment.replace(', ' + guard, '')
        assert guard not in changed_segment
        changed = body[:attribute] + changed_segment + body[member:]
        print('CONTROL ' + ident + ' ' + wire_field + ': remove only ' + guard_name + '; retain Option/default/map capture', flush=True)
        SOURCE.write_text(text[:start] + changed + text[end:])
        try:
            run(ident, None if missing else wire_field, wire_field if missing else None)
        finally:
            SOURCE.write_bytes(ORIGINAL)
            assert SOURCE.read_bytes() == ORIGINAL
    # Exact baseline restored after every mutation. One complete replay supplies
    # all 24 restored passes without recompiling an identical baseline twenty times.
    run()
finally:
    SOURCE.write_bytes(ORIGINAL)
    assert SOURCE.read_bytes() == ORIGINAL
print('24 individually compiled named valid-array failures plus three independent presence-attribute failures; exact restoration after every control; final24 restored native passes.', flush=True)
