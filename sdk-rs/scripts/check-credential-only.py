#!/usr/bin/env python3
"""Credential-only setup bans (docs/plans/RUST_CREDENTIAL_ONLY_SETUP.md, D1/D2).

No consumer may rebuild an empty signing ref or reach the absent/optional
forms of `Identity`. Only `#[cfg(test)]`-gated items are skipped: a `mod x;`
declaration or the brace-matched item that follows the attribute.
"""
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]
EMPTY_REF = re.compile(
    r'KeyRef\((?:String::new\(\)|String::default\(\)|Default::default\(\)|format!\(""\)'
    r'|""\.(?:into|to_string|to_owned)\(\)|String::from\(""\))\)|KeyRef::from\(""\)'
)
# Setup validation metadata: placeholders for supplied-but-empty material.
EMPTY_REF_LINES = {
    ('crypto/src/signer.rs', 'identity.passphrase = Some(KeyRef(String::new()));'),
    ('crypto/src/signer.rs', 'identity.extra.insert(*name, KeyRef(String::new()));'),
}
RULES = [
    ('Identity::signing_opt or set_signing_ref outside setup and construction',
     re.compile(r'\.(?:signing_opt|set_signing_ref)\('),
     {'crypto/src/signer.rs', 'engine/src/signing.rs', 'registry/src/construction.rs'}),
    ('validate_credential outside the declared signing policy',
     re.compile(r'\bvalidate_credential\('),
     {'engine/src/signing.rs'}),
]
TEST_ATTR = re.compile(r'^[ \t]*#\[cfg\(test\)\][ \t]*\n', re.M)


def item_end(text, start):
    """End offset of the item starting at `start` (after its attributes)."""
    i = start
    while True:
        line_end = text.find('\n', i)
        line = text[i:line_end if line_end >= 0 else len(text)].strip()
        if line.startswith('#['):
            i = line_end + 1
            continue
        break
    semi, brace = text.find(';', i), text.find('{', i)
    if brace < 0 or (0 <= semi < brace):
        return semi + 1
    depth, j, quote = 0, brace, False
    while j < len(text):
        c = text[j]
        if quote:
            if c == '\\':
                j += 1
            elif c == '"':
                quote = False
        elif c == '"':
            quote = True
        elif c == '{':
            depth += 1
        elif c == '}':
            depth -= 1
            if depth == 0:
                return j + 1
        j += 1
    return len(text)


def production(text):
    """`text` with every `#[cfg(test)]` item blanked, line numbers kept."""
    out, pos = [], 0
    for match in TEST_ATTR.finditer(text):
        if match.start() < pos:
            continue
        end = item_end(text, match.end())
        out.append(text[pos:match.start()])
        out.append(re.sub(r'[^\n]', ' ', text[match.start():end]))
        pos = end
    out.append(text[pos:])
    return ''.join(out)


def violations(relative, text):
    source = production(text)
    lines = source.split('\n')
    found = []
    for match in EMPTY_REF.finditer(source):
        number = source.count('\n', 0, match.start()) + 1
        if (relative, lines[number - 1].strip()) not in EMPTY_REF_LINES:
            found.append(f'empty KeyRef outside setup validation metadata: {relative}:{number}')
    for description, pattern, allowed in RULES:
        if relative in allowed:
            continue
        for match in pattern.finditer(source):
            found.append(f'{description}: {relative}:{source.count(chr(10), 0, match.start()) + 1}')
    return found


def main():
    found = []
    for path in sorted(ROOT.glob('**/src/**/*.rs')):
        relative = path.relative_to(ROOT).as_posix()
        if 'target' in path.parts or path.name.endswith('_tests.rs'):
            continue
        found += violations(relative, path.read_text())
    for line in found:
        print('BANNED: ' + line)
    return bool(found)


if __name__ == '__main__':
    sys.exit(main())
