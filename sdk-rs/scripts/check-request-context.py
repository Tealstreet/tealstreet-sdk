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


# Owner-authorized initial bootstrap refresh for already-landed #1748 only.
# These exact path maps cannot authorize arbitrary sites or later regrowth.
BOOTSTRAP_PATH = 'venues/paradex/src/history.rs'
BOOTSTRAP_OLD = {'156d12f0af1aa73d997695a9ee5ae2f0b10efdaf05fbf3c5a44b49c0b8c67eb3': 2,
 '2b6c87682e5a05f8f7b1a34a56d5d415f322353ec9c26d679431d57de7aad64b': 1,
 '2c8a5478f2609d37bd412269796524fb8757001af77ca0e1149e8094776b413d': 1,
 '4f41451eb1e1a039f8b647eaee00abfdc9027246d230dff2fad9129e9919c3e7': 1,
 '505f08f644bfce6b593ea547dd3ae8e5d06afb1ddfe5f51c015a1cb6b95cef64': 1,
 '552d45a1b59d0f22ccc9a74886bba18ab8ed7ed63abbbaf21329272a874b4ace': 1,
 '55fe601b0e4d079ab998430cfbafaa7ea570dd124789ffb38859d78b00696d10': 1,
 '5d6691e305370ef1b5e22426e494d28b28936de80d5d5af079e9deae52eb33f2': 2,
 '60ad9d1f92b48483572276a3a75cf6e7c6579a0c8d0118a9f9f51688b851c9aa': 1,
 '664a43c5bfd1ecbc0d29239d7915e542f15fa544270f4b084e9c8f8a0ebd2516': 1,
 '7e6ac08cf11a9f2f3a366e224d2c5df531cef78fd5b215d673548273ddd1fe50': 1,
 '8153debcec98fa16c1dfc5c29c116571f0f4f5735aa73f778c9fafc32448211b': 1,
 '8b8381599d79ae4e48be97e0eb5449852a4d8611cbba96c3dc1eed24a44faa0c': 1,
 'a81744f9afdaf13c609cee0a6b18b04c80c9fa876308896b9d619fd75c8c7b9c': 1,
 'a876e5300951dc42d8c9828d78f0d32d52e42b439cbaf763cc6cdd6b63174865': 1,
 'aa71c1d92e34443bf9b99a67bc669e086e750105b16ed6b6d1835d2e7f2c2c0f': 1,
 'df2ddf42002aefae252f397c268458bbfd87f8511704a888f81560d2d7088665': 1}
BOOTSTRAP_LANDED = {'156d12f0af1aa73d997695a9ee5ae2f0b10efdaf05fbf3c5a44b49c0b8c67eb3': 4,
 '2b6c87682e5a05f8f7b1a34a56d5d415f322353ec9c26d679431d57de7aad64b': 1,
 '4f41451eb1e1a039f8b647eaee00abfdc9027246d230dff2fad9129e9919c3e7': 2,
 '552d45a1b59d0f22ccc9a74886bba18ab8ed7ed63abbbaf21329272a874b4ace': 1,
 '55fe601b0e4d079ab998430cfbafaa7ea570dd124789ffb38859d78b00696d10': 1,
 '60ad9d1f92b48483572276a3a75cf6e7c6579a0c8d0118a9f9f51688b851c9aa': 1,
 '7e6ac08cf11a9f2f3a366e224d2c5df531cef78fd5b215d673548273ddd1fe50': 1,
 '8153debcec98fa16c1dfc5c29c116571f0f4f5735aa73f778c9fafc32448211b': 1,
 'a81744f9afdaf13c609cee0a6b18b04c80c9fa876308896b9d619fd75c8c7b9c': 1,
 'a876e5300951dc42d8c9828d78f0d32d52e42b439cbaf763cc6cdd6b63174865': 1}


# Owner-approved bootstrap maps from actual #1754/#1751/#1752 at
# 5f6488c7664b68cf8b866d2f7f914a3fc99e5fc6. Exact old maps only;
# newer/pruned constraints independently forbid restoring removed sites.
HISTORY_BOOTSTRAPS = {'venues/coinbase/src/history.rs': ({'0451c29993291234cad6e42a586a776c0df200276aac899eaf08c1753d2e83fb': 3,
                                     '644e2a010e2b9cab142812db572645a6eb8617345439cbdb5564ec66bf354645': 2,
                                     '8479aab65a3ff3e83282a3c83d7480252f2d58a32b4a1b3c5434c1d3866caa11': 1,
                                     '8d68e5006e46478875c218af609bebb612700aee1c8a46238c3a7d90ecf9f20d': 1},
                                    {'0451c29993291234cad6e42a586a776c0df200276aac899eaf08c1753d2e83fb': 1,
                                     '76998fe42e6c09630ff50ba44385ed309566a7c45e0b90d584ccbf7534cc0d6e': 1,
                                     '8479aab65a3ff3e83282a3c83d7480252f2d58a32b4a1b3c5434c1d3866caa11': 1}),
 'venues/weex/src/history.rs': ({'2b6c87682e5a05f8f7b1a34a56d5d415f322353ec9c26d679431d57de7aad64b': 1,
                                 '57e9ef562b19c29667a0398bc3f7852967c59c395f72ddf62163007a6d41b0ed': 1,
                                 '5d609353fd1b58bf8e3fe5a6394312bb4a2fd94bfacec1fba9dbcd044854b8e6': 1,
                                 '6d43c66aaf7abcd460dbf1de393805b744df14fbe16199b207541204dfdb892a': 2,
                                 'a71f8e454d2b13bf32e7e5f0480e52dec36d8826551ab16e583fab196fd8a695': 1,
                                 'bbe154e63f6392afb4e62ef9d8cd214a20497c58bcffd3f4811ae22309ede175': 1,
                                 'c179c90dfd7d9612dfa0c8e53858b908acc0a2a21823d27ae3e49397adbbff29': 2,
                                 'c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa': 1,
                                 'df3c1d81952c8a455f966081ae19fcca364d48093c5893e69a82108cac16906d': 4,
                                 'e1915deeaaaaf8a9e3750ca925ab6babf3d80d0e98872e4fa1e3605b0157faf0': 1,
                                 'eedb1e386409cb0cf603aec0da2556ed50ae27b991a5d95dd7803bdcd031fefa': 1,
                                 'fad37049b9b5fba400c6c5026df2ac50477f39d4e606f6edbbf92e51eddb9f7a': 1},
                                {'2b6c87682e5a05f8f7b1a34a56d5d415f322353ec9c26d679431d57de7aad64b': 1,
                                 '57e9ef562b19c29667a0398bc3f7852967c59c395f72ddf62163007a6d41b0ed': 1,
                                 '5d609353fd1b58bf8e3fe5a6394312bb4a2fd94bfacec1fba9dbcd044854b8e6': 1,
                                 '6d43c66aaf7abcd460dbf1de393805b744df14fbe16199b207541204dfdb892a': 1,
                                 'a71f8e454d2b13bf32e7e5f0480e52dec36d8826551ab16e583fab196fd8a695': 1,
                                 'bbe154e63f6392afb4e62ef9d8cd214a20497c58bcffd3f4811ae22309ede175': 2,
                                 'e1915deeaaaaf8a9e3750ca925ab6babf3d80d0e98872e4fa1e3605b0157faf0': 1,
                                 'eedb1e386409cb0cf603aec0da2556ed50ae27b991a5d95dd7803bdcd031fefa': 1,
                                 'fad37049b9b5fba400c6c5026df2ac50477f39d4e606f6edbbf92e51eddb9f7a': 1})}

def subset(counts, baseline):
    return all(count <= baseline.get(digest, 0) for digest, count in counts.items())


def check(current, allowed, previous):
    failures = []
    if current != allowed:
        failures.append('current sites must exactly match the inventory (prune removals; no new sites)')
    for label, baseline in previous:
        for path, counts in allowed.items():
            bound = baseline.get(path, {})
            if path == BOOTSTRAP_PATH and bound == BOOTSTRAP_OLD:
                bound = BOOTSTRAP_LANDED
            if path in HISTORY_BOOTSTRAPS and bound == HISTORY_BOOTSTRAPS[path][0]:
                bound = HISTORY_BOOTSTRAPS[path][1]
            if not subset(counts, bound):
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
