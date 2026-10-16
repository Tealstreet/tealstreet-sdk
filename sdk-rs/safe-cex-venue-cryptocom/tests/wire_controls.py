#!/usr/bin/env python3
"""Sequential source/native controls; object admission controls cover the whole capture."""
import os
import sys
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[3]
CRATE = ROOT / 'sdk-rs/safe-cex-venue-cryptocom'
WIRE = CRATE / 'src/wire.rs'
ORACLE = CRATE / 'tests/types_oracle.cjs'
TS = ROOT / 'packages/safe-cex/src/exchanges/cryptocom/cryptocom.types.ts'
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH'), 'SAFE_CEX_TYPESCRIPT_PATH is required'
COMMAND = ['cargo', 'test', '--manifest-path', str(ROOT / 'sdk-rs/Cargo.toml'),
           '-p', 'safe-cex-venue-cryptocom', '--test', 'types', '--jobs', '1', '--locked']


executed = []


def run():
    return subprocess.run(COMMAND, cwd=ROOT, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)


def passed(result):
    print(result.stdout, flush=True)
    assert result.returncode == 0 and '3 passed; 0 failed' in result.stdout, result.stdout


def control(name, file, mutation, assertion):
    if len(sys.argv) > 1 and name not in sys.argv[1:]:
        return
    original = file.read_bytes()
    try:
        changed = mutation(original.decode())
        assert changed != original.decode(), name
        file.write_text(changed)
        result = run()
        print(result.stdout, flush=True)
        assert result.returncode == 101 and 'test result: FAILED.' in result.stdout and assertion in result.stdout, (name, result.stdout)
        print(f'FAIL observed {name}: {assertion}; exit=101', flush=True)
    finally:
        file.write_bytes(original)
    passed(run())
    print(f'PASS restored {name}', flush=True)
    executed.append(name)


def swap(record, left, right):
    def mutate(source):
        start = source.index(f'struct Wire{record} {{')
        end = source.index('\n}', start)
        section = source[start:end]
        old_left = f'#[serde(rename = "{left}")]'
        old_right = f'#[serde(rename = "{right}")]'
        assert section.count(old_left) == section.count(old_right) == 1
        changed = section.replace(old_left, 'SWAP_SENTINEL').replace(old_right, old_left).replace('SWAP_SENTINEL', old_right)
        return source[:start] + changed + source[end:]
    return mutate


def decode_mutation(record, tail):
    def mutate(source):
        marker = "impl<'de> Deserialize<'de> for " + record + ' {'
        start = source.index(marker)
        end = source.index('\n}', start) + 2
        begin = source.index('Wire' + record + '::deserialize(', start)
        opening = source.index('(', begin)
        depth = 1
        close = opening + 1
        while depth:
            depth += (source[close] == '(') - (source[close] == ')')
            close += 1
        assert close < end
        return source[:close] + tail + source[close:]
    return mutate


def admit_array(record):
    """Exercise the complete object guard, including the outer numeric capture."""
    def mutate(source):
        import re
        marker = "impl<'de> Deserialize<'de> for " + record + ' {'
        start = source.index(marker)
        end = source.index('\n}', start) + 2
        section = source[start:end]
        opening = section.index('{', section.index('fn deserialize')) + 1
        closing = section.rindex('\n    }')
        body = section[opening:closing]
        capture = body.index('crate::wire_record::capture(')
        key_start = body.index('&[', capture)
        key_end = body.index(']', key_start)
        keys = re.findall(r'"([^"]+)"', body[key_start:key_end])
        assert keys
        literal = ', '.join('"' + key + '"' for key in keys)
        wrong = r'''
        let captured = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        let observed = safe_cex_num::numeric_ingress::js_capture_output(&captured).map_err(serde::de::Error::custom)?;
        if let safe_cex_types::retained_json::RetainedJsonInput::Array(values) = observed {
            let names = [KEYS];
            let object = safe_cex_types::retained_json::RetainedJsonInput::Object(
                names.into_iter().zip(values).map(|(key, value)| (key.to_owned(), value)).collect()
            ).into_value().map_err(serde::de::Error::custom)?;
            let text = serde_json::to_string(&object).map_err(serde::de::Error::custom)?;
            return safe_cex_num::numeric_ingress::js_from_str::<Self>(&text).map_err(serde::de::Error::custom);
        }
        captured.decode_with(|deserializer| {
BODY
        }).map_err(serde::de::Error::custom)
'''.replace('KEYS', literal).replace('BODY', body)
        return source[:start] + section[:opening] + wrong + section[closing:] + source[end:]
    return mutate


passed(run())
for record, left, right in [
    ('CryptocomInstrument', 'tradable', 'beta_product'),
    ('CryptocomTicker', 'b', 'k'),
    ('CryptocomCandlestick', 'o', 'h'),
    ('CryptocomPositionBalance', 'quantity', 'reserved_qty'),
]:
    control(record, WIRE, swap(record, left, right), f'decoded_field:{record}:{left}')
control('CryptocomTrade', WIRE, swap('CryptocomTrade', 'p', 'q'),
        'wire_ingress:CryptocomTrade:numeric ingress changed Number bits at $.p')
control('CryptocomError', WIRE,
        decode_mutation('CryptocomError', '.map(|mut record| { record.message.push_str("wrong"); record })'),
        'decoded_field:CryptocomError:message')

def narrow_trade_side(source):
    start = source.index('export interface CryptocomTrade {')
    end = source.index('\n}', start)
    section = source[start:end]
    assert section.count("s: 'BUY' | 'SELL'") == 1
    return source[:start] + section.replace("s: 'BUY' | 'SELL'", "s: 'BUY'") + source[end:]


control('trade_side_source_narrowing', TS, narrow_trade_side, 'trade_side_literal_census')
control('trade_side_symmetric_rename', WIRE,
        lambda text: text.replace('#[serde(rename = "BUY")]', 'SIDE_SWAP')
                         .replace('#[serde(rename = "SELL")]', '#[serde(rename = "BUY")]')
                         .replace('SIDE_SWAP', '#[serde(rename = "SELL")]'),
        'decoded_field:CryptocomTrade:s')
def account_decode_mutation(record, tail=None):
    return decode_mutation(record, tail) if tail else admit_array(record)


control('CryptocomAccountsResponse', WIRE,
        account_decode_mutation('CryptocomAccountsResponse', '.map(|mut record| { record.this_account.uuid.push_str("wrong"); record })'),
        'decoded_field:CryptocomAccountsResponse:this_account.uuid')
control('account_outer_guard', WIRE, account_decode_mutation('CryptocomAccountsResponse'),
        'account_populated_array_refusal:outer')
control('account_inner_guard', WIRE, account_decode_mutation('CryptocomAccountIdentity'),
        'account_populated_array_refusal:inner')
control('account_nested_census', ORACLE,
        lambda text: text.replace('checker.getPropertiesOfType(fieldType).map(child => describe(child))', '[]'),
        'account_shape_census')
control('CryptocomBalanceSummary', WIRE,
        swap('CryptocomBalanceSummary', 'total_available_balance', 'total_margin_balance'),
        'decoded_field:CryptocomBalanceSummary:total_available_balance')
control('summary_entry_collapse', WIRE,
        account_decode_mutation('CryptocomBalanceSummary', '.map(|mut record| { if record.position_balances.len() > 1 { record.position_balances[1] = record.position_balances[0].clone(); } record })'),
        'decoded_field:CryptocomBalanceSummary:position_balances')
control('summary_object_guard', WIRE, account_decode_mutation('CryptocomBalanceSummary'),
        'summary_populated_array_refusal')
control('summary_array_census', ORACLE,
        lambda text: text.replace("return [property.name, 'array', schema(target.name.text, nextAncestors)];", "return [property.name, 'array', []];"),
        'summary_array_reference_census')
def book_decode_mutation(record, tail=None):
    return decode_mutation(record, tail) if tail else admit_array(record)


control('CryptocomBook', WIRE, swap('CryptocomBookData', 'bids', 'asks'),
        'expected_native_count:book:bids')
control('book_tuple_swap', WIRE,
        book_decode_mutation('CryptocomBookData', '.map(|mut record| { if let Some(tuple) = record.bids.first_mut() { tuple.swap(0, 1); } record })'),
        'decoded_field:CryptocomBook:data')
control('book_outer_guard', WIRE, book_decode_mutation('CryptocomBook'),
        'book_populated_array_refusal:outer')
control('book_inner_guard', WIRE, book_decode_mutation('CryptocomBookData'),
        'book_populated_array_refusal:inner')
control('book_numeric_corruption', WIRE,
        book_decode_mutation('CryptocomBook', '.map(|mut record| { record.depth = 0.0; record })'),
        'wire_ingress:CryptocomBook:numeric ingress changed Number bits at $.depth')
control('book_source_array_shadow', TS,
        lambda text: text.replace('export interface CryptocomBook {', 'type Array<T> = T;\nexport interface CryptocomBook {'),
        'array_builtin_shadow:CryptocomBook:data')
control('book_tuple_census', ORACLE,
        lambda text: text.replace("return [property.name, 'tuple-array', kinds];", "return [property.name, 'tuple-array', kinds.slice(0,2)];"),
        'book_compiler_shape_census')
control('CryptocomTradeFill', WIRE, swap('CryptocomTradeFill', 'taker_side', 'side'),
        'decoded_field:CryptocomTradeFill:taker_side')
control('fill_liquidity_symmetric_rename', WIRE,
        lambda text: text.replace('#[serde(rename = "MAKER")]', 'LIQUIDITY_SWAP')
                         .replace('#[serde(rename = "TAKER")]', '#[serde(rename = "MAKER")]')
                         .replace('LIQUIDITY_SWAP', '#[serde(rename = "TAKER")]'),
        'decoded_field:CryptocomTradeFill:liquidity_indicator')
control('fill_optional_null_admission', WIRE,
        lambda text: text.replace('String::deserialize(deserializer).map(Some)', 'Option::<String>::deserialize(deserializer)'),
        'trade_fill_optional_kind_refusal')
control('fill_object_guard', WIRE, book_decode_mutation('CryptocomTradeFill'),
        'trade_fill_populated_array_refusal')
control('fill_optional_source_narrowing', TS,
        lambda text: text.replace('  trade_match_id: string;\n  client_oid?: string;', '  trade_match_id: string;\n  client_oid: string;'),
        'trade_fill_optional_census')
control('fill_literal_source_narrowing', TS,
        lambda text: text.replace("liquidity_indicator: 'MAKER' | 'TAKER'", "liquidity_indicator: 'MAKER'"),
        'trade_fill_literal_census:liquidity_indicator')
control('fill_nullable_source_widening', TS,
        lambda text: text.replace('  trade_match_id: string;\n  client_oid?: string;', '  trade_match_id: string;\n  client_oid?: string | null;'),
        'field_kind:CryptocomTradeFill:client_oid')
control('heritage_blind_census', ORACLE,
        lambda text: text.replace('checker.getPropertiesOfType(type).map(property =>',
                                  'checker.getPropertiesOfType(type).filter(property => property.valueDeclaration.parent === node).map(property =>'),
        'inherited_required_field_control')
control('object_guard_removal', WIRE, admit_array('CryptocomInstrument'),
        'populated_array_refusal:CryptocomInstrument')
control('numeric_decoder_corruption', WIRE,
        decode_mutation('CryptocomError', '.map(|mut record| { record.code = 0.0; record })'),
        'wire_ingress:CryptocomError:numeric ingress changed Number bits at $.code')
control('merged_declaration_blind_census', ORACLE,
        lambda text: text.replace('  assert.equal(declarations.length, 1, `interface_declaration_count:${name}`);', ''),
        'merged_declaration_refusal_control')
control('merged_parent_blind_census', ORACLE,
        lambda text: text.replace('      assert.equal(parents.length, 1, `heritage_declaration_count:${name}`);', ''),
        'merged_parent_refusal_control')
control('unresolved_heritage_blind_census', ORACLE,
        lambda text: text.replace('for (const clause of declaration.heritageClauses || [])', 'for (const clause of [])'),
        'unresolved_heritage_refusal_control')
assert set(executed) == set(sys.argv[1:]) if len(sys.argv) > 1 else len(executed) == 36, executed
print('requested named controls passed' if len(sys.argv) > 1 else '24 production controls and 12 census controls failed by name and passed after restoration', flush=True)
