#!/usr/bin/env python3
"""Credential-only setup bans (docs/plans/RUST_CREDENTIAL_ONLY_SETUP.md, D1/D2).

No consumer may rebuild an empty signing ref or reach the absent/optional
forms of `Identity`. Only `#[cfg(test)]` modules (a `mod x;` declaration or a
brace-matched inline module) and `*_tests.rs` files are skipped.
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
    ('install_scalar_only_primary outside trusted local setup',
     re.compile(r'\binstall_scalar_only_primary\('),
     {'crypto/src/signer.rs', 'engine/src/signing.rs'}),
    ('seal_public_key outside trusted local setup',
     re.compile(r'\bseal_public_key\('),
     {'crypto/src/signer.rs', 'engine/src/signing.rs'}),
    ('Identity::public_key_sealed_by outside setup and the engine',
     re.compile(r'\bpublic_key_sealed_by\('),
     {'engine/src/signing.rs', 'engine/src/engine.rs', 'registry/src/paradex_setup.rs'}),
    ('Signer::incarnation implemented outside the trusted local provider',
     re.compile(r'\bfn\s+incarnation\s*\('),
     {'crypto/src/signer.rs', 'engine/src/signing.rs'}),
    ('validate_credential outside the declared signing policy',
     re.compile(r'\bvalidate_credential\('),
     {'engine/src/signing.rs'}),
]
TEST_ATTR = re.compile(r'^[ \t]*#\[cfg\(test\)\][ \t]*\n', re.M)


RAW_STRING = re.compile(r'r(#*)"')
CHAR = re.compile(r"'(?:\\.|[^\\'\n])'")
TEST_MODULE = re.compile(r'(?:pub(?:\([^)]*\))?\s+)?mod\s+\w+\s*([;{])')


def skip_literal(text, j):
    """Offset just past a string or raw-string literal starting at `j`."""
    raw = RAW_STRING.match(text, j)
    if raw:
        close = text.find('"' + raw.group(1), raw.end())
        return len(text) if close < 0 else close + 1 + len(raw.group(1))
    j += 1
    while j < len(text) and text[j] != '"':
        j += 2 if text[j] == '\\' else 1
    return j + 1


def module_end(text, start):
    """End of the `#[cfg(test)]` module at `start`, or None if it gates anything else."""
    i = start
    while text[i:].lstrip().startswith('#['):
        i = text.index('\n', i) + 1
    module = TEST_MODULE.match(text[i:].lstrip())
    if not module:
        return None
    i += len(text[i:]) - len(text[i:].lstrip())
    if module.group(1) == ';':
        return i + module.end()
    depth, j = 0, i + module.end() - 1
    while j < len(text):
        c = text[j]
        if c == '"' or RAW_STRING.match(text, j):
            j = skip_literal(text, j)
            continue
        char = CHAR.match(text, j) if c == "'" else None
        if char:
            j = char.end()
            continue
        if c == '{':
            depth += 1
        elif c == '}':
            depth -= 1
            if depth == 0:
                return j + 1
        j += 1
    return len(text)


def production(text):
    """`text` with every `#[cfg(test)]` module blanked, line numbers kept.
    Other gated items are scanned: tests needing banned forms belong in test
    modules or `*_tests.rs` files."""
    out, pos = [], 0
    for match in TEST_ATTR.finditer(text):
        if match.start() < pos:
            continue
        end = module_end(text, match.end())
        if end is None:
            continue
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
