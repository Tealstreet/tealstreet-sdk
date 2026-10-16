from pathlib import Path
import re
import subprocess


CRATE = Path(__file__).resolve().parents[1]
ROOT = CRATE.parents[1]
TABLES = (
    ('types.rs', 'ENDPOINTS'), ('types.rs', 'INTERVAL'), ('types.rs', 'WS_INTERVAL'),
    ('types.rs', 'POSITION_SIDE'), ('types.rs', 'ORDER_STATUS'), ('types.rs', 'ORDER_TYPE'),
    ('types.rs', 'ORDER_SIDE'), ('types.rs', 'TIME_IN_FORCE'),
    ('spot_types.rs', 'KUCOIN_SPOT_ENDPOINTS'), ('spot_types.rs', 'KUCOIN_SPOT_INTERVAL'),
    ('spot_types.rs', 'KUCOIN_SPOT_ORDER_STATUS'), ('spot_types.rs', 'KUCOIN_SPOT_ORDER_TYPE'),
    ('spot_types.rs', 'KUCOIN_SPOT_TIME_IN_FORCE'),
)


def run(assertion=None):
    result = subprocess.run(
        ['cargo', 'test', '--locked', '--manifest-path', str(ROOT / 'sdk-rs/Cargo.toml'),
         '-p', 'safe-cex-venue-kucoin', '--jobs', '1', '--test', 'tables',
         'live_typescript_tables', '--', '--exact'], cwd=ROOT, text=True,
        stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    expected = 101 if assertion else 0
    if assertion and 'test live_typescript_tables ... FAILED' not in result.stdout:
        raise RuntimeError('control did not reach compiled test\n' + result.stdout)
    if result.returncode != expected or (assertion and assertion not in result.stdout):
        raise RuntimeError(f'{result.returncode}\n{result.stdout}')
    print(f'{expected}: {assertion or "restored"}', flush=True)


def mutate(file, name, mode, bypass):
    source = CRATE / 'src' / file
    test = CRATE / 'tests/tables.rs'
    original, checks = source.read_bytes(), test.read_bytes()
    try:
        text = original.decode()
        match = re.search(r'(pub const ' + name + r':.*?=\s*&\[)(.*?)(\];)', text, re.S)
        if not match:
            raise RuntimeError(f'missing table {name}')
        pair = re.search(r'\([^)]*\)', match[2])[0]
        if mode == 'duplicate':
            body = pair + ',' + match[2]
        elif mode == 'reverse':
            body = ','.join(reversed(re.findall(r'\([^)]*\)', match[2]))) + ','
        elif mode == 'empty':
            body = ''
        elif mode == 'partial':
            body = pair + ','
        elif mode == 'extra':
            body = match[2] + '("cardinality-extra", OrderType::Limit),'
        else:
            raise RuntimeError(mode)
        source.write_text(text[:match.start(2)] + body + text[match.end(2):])
        if bypass:
            adjusted = checks.decode()
            anchor = '    assert_eq!(\n        pairs.len(),\n        expected,\n        "KUCOIN-TABLE {name} native pair count"\n    );'
            if adjusted.count(anchor) != 1:
                raise RuntimeError('missing length guard anchor')
            adjusted = adjusted.replace(anchor, '    assert!(expected > 0);')
            if bypass == 'both':
                start = adjusted.index('    let unique = pairs')
                end = adjusted.index('\n}', start)
                adjusted = adjusted[:start] + '    assert!(!name.is_empty());\n    let _pairs = pairs;' + adjusted[end:]
            test.write_text(adjusted)
        assertion = ('KUCOIN-TABLE ORDER_TYPE ordered pairs' if bypass == 'both' else
                     f'KUCOIN-TABLE {name} native ' + ('unique keys' if bypass else 'pair count'))
        print(f'{name} {mode} bypass={bypass}', flush=True)
        run(assertion)
    finally:
        source.write_bytes(original)
        test.write_bytes(checks)


def main():
    for file, name in TABLES:
        mutate(file, name, 'duplicate', None)
    mutate('types.rs', 'ORDER_TYPE', 'duplicate', 'length')
    for mode in ('duplicate', 'empty', 'partial', 'extra', 'reverse'):
        mutate('types.rs', 'ORDER_TYPE', mode, 'both')
    run()


if __name__ == '__main__':
    main()
