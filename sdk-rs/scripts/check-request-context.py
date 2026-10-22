#!/usr/bin/env python3
"""Exact, shrinking inventory of opaque venue HTTP/completion provenance.

This is a conservative lexical ban, not Rust name resolution or semantic
classification. Every matching member/qualified reference in venue src (including
tests) is frozen. It confers no managed retry, release or outcome guarantees.
"""
import collections
import hashlib
import json
import os
import pathlib
import re
import subprocess

ROOT = pathlib.Path(__file__).resolve().parents[1]
MANIFEST = 'scripts/request-legacy-sites.json'
METHODS = {'request', 'request_streamed', 'request_streamed_body', 'sign_http', 'done', 'fail'}
IDENT = re.compile(r'[A-Za-z_][A-Za-z_0-9]*')
RAW = re.compile(r'(?:br|cr|r)(\#*)"')
CHAR = re.compile(r"(?:b)?'(?:\\(?:x[0-9a-fA-F]{2}|u\{[0-9a-fA-F_]+\}|.)|[^'\\\r\n])'")


def tokens(source):
    """Keep literal bytes atomic; ignore nested comments and formatting."""
    result = []
    i = 0
    while i < len(source):
        if source[i].isspace():
            i += 1
        elif source.startswith('//', i):
            end = source.find('\n', i + 2)
            i = len(source) if end < 0 else end + 1
        elif source.startswith('/*', i):
            depth = 1
            i += 2
            while depth and i < len(source):
                if source.startswith('/*', i):
                    depth += 1
                    i += 2
                elif source.startswith('*/', i):
                    depth -= 1
                    i += 2
                else:
                    i += 1
            if depth:
                raise ValueError('unterminated Rust comment')
        else:
            start = i
            raw = RAW.match(source, i)
            char = CHAR.match(source, i)
            if raw:
                close = '"' + raw[1]
                end = source.find(close, raw.end())
                if end < 0:
                    raise ValueError('unterminated Rust raw string')
                i = end + len(close)
            elif source[i] == '"' or source.startswith(('b"', 'c"'), i):
                i += 1 if source[i] == '"' else 2
                while i < len(source) and source[i] != '"':
                    i += 2 if source[i] == '\\' else 1
                if i >= len(source):
                    raise ValueError('unterminated Rust string')
                i += 1
            elif char:
                i = char.end()
            elif source.startswith('r#', i) and (match := IDENT.match(source, i + 2)):
                i = match.end()
            elif match := IDENT.match(source, i):
                i = match.end()
            else:
                i += 2 if source.startswith('::', i) else 1
            result.append(source[start:i])
    return result


PAIRS = {'(': ')', '[': ']', '{': '}', '<': '>'}


def group_end(stream, start):
    close = PAIRS[stream[start]]
    end = start + 1
    while end < len(stream):
        if stream[end] == close:
            return end + 1
        # Angles inside arguments may be comparisons, not generic brackets.
        if stream[end] in PAIRS and (stream[end] != '<' or stream[start] == '<'):
            end = group_end(stream, end)
        else:
            end += 1
    raise ValueError('unbalanced legacy expression')


def receiver_start(stream, member):
    def atom(end):
        if end < 0:
            raise ValueError('missing legacy receiver')
        token = stream[end]
        if token == '?':
            return atom(end - 1)
        if token in PAIRS.values():
            opening = next(key for key, value in PAIRS.items() if value == token)
            depth = 1
            start = end - 1
            while depth and start >= 0:
                depth += (stream[start] == token) - (stream[start] == opening)
                start -= 1
            if depth:
                raise ValueError('unbalanced legacy receiver')
            start += 1
            # Function calls, indexing and generic receivers include their name.
            if start and (IDENT.fullmatch(stream[start - 1]) or stream[start - 1] in {')', ']'}) and stream[start - 1] not in {'return', 'break', 'yield'}:
                return atom(start - 1)
            return start
        if IDENT.fullmatch(token) or token.isdecimal() or token.startswith('r#') or token.startswith('$'):
            return end
        raise ValueError('invalid legacy receiver')

    start = atom(member - 2)
    while start >= 2 and stream[start - 1] in {'.', '::'}:
        start = atom(start - 2)
    if start and stream[start - 1] == '$':
        start -= 1
    return start


def fingerprints(source):
    stream = tokens(source)
    found = collections.Counter()
    definitions = []
    for i, token in enumerate(stream):
        if token == 'macro_rules' and stream[i + 1:i + 2] == ['!']:
            opening = i + 3
            if opening >= len(stream) or stream[opening] not in {'(', '[', '{'}:
                raise ValueError('invalid macro definition')
            definitions.append((i, group_end(stream, opening)))

    def add(start, end):
        payload = json.dumps(stream[start:end], ensure_ascii=False, separators=(',', ':'))
        found[hashlib.sha256(payload.encode()).hexdigest()] += 1

    for i, token in enumerate(stream):
        name = token.removeprefix('r#')
        member = i and stream[i - 1] in {'.', '::'} and (i < 2 or stream[i - 2] != '.')
        indirect = token == '$' and member
        if (name in METHODS and member) or indirect:
            # No current venue macro generates opaque calls. Reject that shape
            # instead of counting one definition as an arbitrary number of uses.
            if any(start <= i < end for start, end in definitions):
                raise ValueError('legacy provenance under macro definition')
            end = i + (2 if indirect else 1)
            if stream[end:end + 2] == ['::', '<']:
                end = group_end(stream, end + 1)
            if stream[end:end + 1] == ['(']:
                end = group_end(stream, end)
            add(receiver_start(stream, i), end)
        # Macro parameters can select a method, and include! can insert Rust
        # outside the scanned files. Freeze both escape shapes conservatively.
        if stream[i + 1:i + 2] == ['!'] and stream[i + 2:i + 3] in [['('], ['{'], ['[']]:
            end = group_end(stream, i + 2)
            if name == 'include':
                raise ValueError('unscanned Rust include')
            if any(t.removeprefix('r#') in METHODS for t in stream[i + 3:end]):
                add(i, end)
    return found


def sites():
    return {str(path.relative_to(ROOT)): dict(counts)
            for path in sorted((ROOT / 'venues').glob('*/src/**/*.rs'))
            if (counts := fingerprints(path.read_text()))}


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError('duplicate manifest key')
        result[key] = value
    return result


def manifest(text):
    value = json.loads(text, object_pairs_hook=unique_object)
    if not isinstance(value, dict):
        raise ValueError('manifest must be a path/count map')
    for path, counts in value.items():
        if (not re.fullmatch(r'venues/[A-Za-z0-9_-]+/src/.+\.rs', path)
                or '..' in path.split('/') or not isinstance(counts, dict) or not counts):
            raise ValueError('invalid manifest path/counts')
        if any(not re.fullmatch(r'[a-f0-9]{64}', digest) or type(count) is not int or count <= 0
               for digest, count in counts.items()):
            raise ValueError('invalid manifest fingerprint/count')
    return value


def check(current, allowed, previous):
    failures = []
    if current != allowed:
        failures.append('current sites must exactly match the inventory (prune removals; no new sites)')
    for label, baseline in previous:
        for path, counts in allowed.items():
            if any(count > baseline.get(path, {}).get(digest, 0) for digest, count in counts.items()):
                failures.append(f'{path}: inventory grew against {label}')
    return failures


def git(*args):
    return subprocess.run(['git', '-C', str(ROOT), *args], text=True, capture_output=True, check=True).stdout


def previous_manifests():
    base = os.environ.get('GITHUB_BASE_REF') or 'feat/rust-rewrite'
    parents = git('rev-list', '--parents', '-n', '1', 'HEAD').split()[1:]
    refs = ['HEAD', *parents, f'refs/remotes/origin/{base}']
    if event_file := os.environ.get('GITHUB_EVENT_PATH'):
        event = json.loads(pathlib.Path(event_file).read_text())
        if 'pull_request' in event:
            # The branch ref may move; the event's base is immutable provenance.
            exact = event['pull_request']['base']['sha']
            if not re.fullmatch(r'[a-f0-9]{40}', exact):
                raise ValueError('invalid event base commit')
            refs.append(exact)
    relative = git('rev-parse', '--show-prefix').strip() + MANIFEST
    previous = []
    for ref in refs:
        # A missing ref is not a missing manifest. CI must fetch history first.
        git('rev-parse', '--verify', f'{ref}^{{commit}}')
        files = git('ls-tree', '--full-name', '--name-only', ref, '--', MANIFEST).splitlines()
        if relative in files:
            previous.append((ref, manifest(git('show', f'{ref}:{relative}'))))
    return previous


def main():
    try:
        failures = check(sites(), manifest((ROOT / MANIFEST).read_text()), previous_manifests())
    except (ValueError, OSError, KeyError, TypeError, subprocess.CalledProcessError) as error:
        # Never print source, literal contents or subprocess stdout/stderr.
        failures = [f'cannot verify legacy inventory: {type(error).__name__}']
    for failure in failures:
        print('BANNED: request provenance: ' + failure)
    return bool(failures)


if __name__ == '__main__':
    raise SystemExit(main())
