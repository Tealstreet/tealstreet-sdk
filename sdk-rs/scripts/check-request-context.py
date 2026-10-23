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
                                 'fad37049b9b5fba400c6c5026df2ac50477f39d4e606f6edbbf92e51eddb9f7a': 1}),
 'venues/cryptocom/src/history.rs': ({'073a9daacf5c0553bfb2650ddc8db7b942b5f6a0c8125261daff7479814953b7': 1,
                                      '15aa6550af43157e2860fe050e427fdb2e8d6f1f162eaa9b26a3532da4bb3aca': 2,
                                      '24076834daaed90b9975244bc5a42e97598ac44f0e22fcf8ca429a98248cbce4': 1,
                                      '2b6c87682e5a05f8f7b1a34a56d5d415f322353ec9c26d679431d57de7aad64b': 1,
                                      '4d39b6c1f48b4fe3cd73a278a44738b56c2d22a36fa0a56df1692009069d7a69': 1,
                                      '5d6691e305370ef1b5e22426e494d28b28936de80d5d5af079e9deae52eb33f2': 1,
                                      '5f1c224f8aeec44e64bda630f879b41c6706957631dee106263ed3ec48c1beb5': 1,
                                      '7698d5bd23a40574165507b4b54b5d093fb00701512d29250192f1345e4da553': 1,
                                      '868b4a9af72a4afc961dcd2ad5bbd3601cd82824b125da57ed4b80ec47fce000': 1,
                                      'a7138869c3f8c008632575093250e9e8c8821fff67b2b00e1deef8a8b768ea72': 1,
                                      'ad251e5c58e1d420c73ffe2316dc86c18d35f699fb86b25eba7ff188a9594df4': 1,
                                      'd702253b3d082dc8c7278bfe17bdb876eeda775fedd004ebaaeea6cdd16f24ad': 2,
                                      'ed348ebd518ad9c243df60c4fe07438a9f3ad6ac777ed07eb88af7d20347060e': 1},
                                     {'073a9daacf5c0553bfb2650ddc8db7b942b5f6a0c8125261daff7479814953b7': 1,
                                      '2b6c87682e5a05f8f7b1a34a56d5d415f322353ec9c26d679431d57de7aad64b': 1,
                                      '4d39b6c1f48b4fe3cd73a278a44738b56c2d22a36fa0a56df1692009069d7a69': 1,
                                      '5f1c224f8aeec44e64bda630f879b41c6706957631dee106263ed3ec48c1beb5': 1,
                                      '7698d5bd23a40574165507b4b54b5d093fb00701512d29250192f1345e4da553': 1,
                                      '868b4a9af72a4afc961dcd2ad5bbd3601cd82824b125da57ed4b80ec47fce000': 1,
                                      'ad251e5c58e1d420c73ffe2316dc86c18d35f699fb86b25eba7ff188a9594df4': 1,
                                      'd702253b3d082dc8c7278bfe17bdb876eeda775fedd004ebaaeea6cdd16f24ad': 2,
                                      'dfb17bdf89f16befd8bb7566ac05d0bddf2980c6627b642f20e1f1b5a4859508': 1}),
 'venues/grvt/src/history.rs': ({'0451c29993291234cad6e42a586a776c0df200276aac899eaf08c1753d2e83fb': 1,
                                 '0bc1914737ba4f26b1f4a43a7db513f0c98aafaa24bd482b6cb5b2e21ef9d704': 1,
                                 '43e76c2e596d56215f4ecd2f1cd300980b527f24c83ec67fbc6a19d6e78c81b9': 1,
                                 '5806755b2e4365a0cce4c02f6f5a15ec7fab6beaa6b0218ae5ccfa2fb93a8892': 1,
                                 '625ef3cfd881af1a997b7e309af2712fabf1013668ca5ca403d4a59913d01d4d': 1,
                                 '755a658b5cfaa9ec659ee94f62b5f535e94a4a91391476bda23ef1199af846bf': 2,
                                 '83b8e68cd8445af7475b790f03a56ef5d72018274c5416a79fcab14f7f58cd7d': 1,
                                 '9d3eea4ddeb346d10f8ebebfbc4dbbcde5939983ebcb9e2530b7c7e718be6e17': 1,
                                 'd5e898e07cc7c2b9f7927c0f9a7f462214e004fdec265ff4f3ed0370a6db1a81': 2,
                                 'd714610c15915805a059c4d8796cf1954a2908722189ac570a2c127a55963ee7': 1,
                                 'f43453f19ab26af8ad6edee4529387f502a76b04b80658aac68a19e1034c4904': 1},
                                {'0451c29993291234cad6e42a586a776c0df200276aac899eaf08c1753d2e83fb': 1,
                                 '0bc1914737ba4f26b1f4a43a7db513f0c98aafaa24bd482b6cb5b2e21ef9d704': 1,
                                 '43e76c2e596d56215f4ecd2f1cd300980b527f24c83ec67fbc6a19d6e78c81b9': 1,
                                 '5694172a506b48e37a66cc80e38560361ec75b97447d4a687ae99811e66fd516': 1,
                                 '5806755b2e4365a0cce4c02f6f5a15ec7fab6beaa6b0218ae5ccfa2fb93a8892': 1,
                                 '625ef3cfd881af1a997b7e309af2712fabf1013668ca5ca403d4a59913d01d4d': 1,
                                 '706e12027f633d2c581b43201426e9539f4b9ecd03159693d03873420a41cd61': 1,
                                 '83b8e68cd8445af7475b790f03a56ef5d72018274c5416a79fcab14f7f58cd7d': 1,
                                 '9d3eea4ddeb346d10f8ebebfbc4dbbcde5939983ebcb9e2530b7c7e718be6e17': 1,
                                 'b075739674bf080c2089eea7c5fb0d89b2aea115d27b532da2ff55f1c32280c1': 1,
                                 'd714610c15915805a059c4d8796cf1954a2908722189ac570a2c127a55963ee7': 1}),
 # Exact same-site history re-expression; no new HTTP recipe or authority.
 'venues/hyperliquid/src/history.rs': ({'0fe010fce7523ee969ea42d37cdce2a68a5521ba7aca8cea0bf1232c80fba943': 1,
  '262a8e069a1b733cc58b211a154efc58e29e39de18f94bf8df269a2f8dcd6666': 1,
  '3df680cd1e2f4c54c571665d0801de18a9503305bd36f985f4e874ddca464ab3': 1,
  '4a595f0e8169456b0ddf61d405d836b0cd131fe7329ad6aadc94049f9a2e37d9': 1,
  '505f08f644bfce6b593ea547dd3ae8e5d06afb1ddfe5f51c015a1cb6b95cef64': 1,
  '5d6691e305370ef1b5e22426e494d28b28936de80d5d5af079e9deae52eb33f2': 3,
  '639af94fb6af5ac86cbf14ae13882f3b71a947bcc6c7afa0e7868179b1ef182a': 1,
  '63c672fa5285db3d634c9d5ea404ef6866a6279f8c279f95d484874402e2d2f5': 1,
  '70af857378d4296e4191c7baf9894fecedb339396eddbfc5cf6ffc1a492f1acd': 1,
  '95ca4237828f8435d55e2d1e464bad969c28ac05efed94d2a1de945845738d75': 1,
  'aa71c1d92e34443bf9b99a67bc669e086e750105b16ed6b6d1835d2e7f2c2c0f': 2,
  'ad251e5c58e1d420c73ffe2316dc86c18d35f699fb86b25eba7ff188a9594df4': 1,
  'b1854c6f5b64c99469571ff9e2f7e1da0c69bf23768ad9537e12567cf7473aa9': 3,
  'c5a021da6919a28b758460074e6af502c4c6d47cf4f00443bd987daccb600548': 1,
  'c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa': 1,
  'da539b025b1000762173bebe5cbd496d99e02923664484e6f40e183ee7ff9118': 1},
 {'180d8a106dd44bf339fbb2be6079affef61e703e71477572a5b804ffa76b1c0b': 2,
  '3df680cd1e2f4c54c571665d0801de18a9503305bd36f985f4e874ddca464ab3': 1,
  '4a595f0e8169456b0ddf61d405d836b0cd131fe7329ad6aadc94049f9a2e37d9': 1,
  '70af857378d4296e4191c7baf9894fecedb339396eddbfc5cf6ffc1a492f1acd': 1,
  '7658b4ae003384019ae0454fa74a2ca07a7379da26a0c633e0e088ef98db9a4e': 1,
  '95ca4237828f8435d55e2d1e464bad969c28ac05efed94d2a1de945845738d75': 1,
  'a8154036fb27748faf81b7747c40e8362162b5271d8c5979eba20d6b8d766ef0': 1,
  'ad251e5c58e1d420c73ffe2316dc86c18d35f699fb86b25eba7ff188a9594df4': 1,
  'b1854c6f5b64c99469571ff9e2f7e1da0c69bf23768ad9537e12567cf7473aa9': 2,
  'c5a021da6919a28b758460074e6af502c4c6d47cf4f00443bd987daccb600548': 1,
  'c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa': 1,
  'da539b025b1000762173bebe5cbd496d99e02923664484e6f40e183ee7ff9118': 1,
  'e6b5e06ea1d1144f3419909a5caa2040fb5042168a325cc09e7997675f738e62': 1}),
 # Owner-approved KuCoin HistoryRows re-expression: same GET site, fewer legacy sites.
 'venues/kucoin/src/spot/mod.rs': ({'06eab8c4791cec49c676233d2e64048f1eaefe972dddfc52dfdffa06b4240730': 1,
  '08746e27d5c5b4fa82d1534a206c0f5932b85da2bc45429c8e9bd888544ebff4': 1,
  '0affdf7db22f36fd9b316e39cd8b2600da1d6fa62831571a986060fa14f341b3': 1,
  '2485f9b0677fc07b9dc4f3202a0dbfc7dd3b13e82476cd509be3282e8ce4d863': 1,
  '2b6c87682e5a05f8f7b1a34a56d5d415f322353ec9c26d679431d57de7aad64b': 2,
  '417cd86311e6261c7b1911896dc467829c4cfafd1fc48956cfbce153fbc17582': 1,
  '44284ddcfc94ad5da1cd7f5576dc01737ad7eeb0f75aa49c95bf42849d08790a': 1,
  '4d14eae20fff4dab766584ec10ba890cc51f49fbf86cf1e5d95860110212dbf1': 1,
  '520659ca128f10f1f3f1a724ed704b116b8f56da364dcd28fd6aaabfc25c5872': 1,
  '96134b6210eed5e21fe96c0412c596ffa47622ce39c4f6247960efe6dec683a2': 1,
  '97e495223a50ee0b0214ca3f811598e16b758b9bb3eb780db321e78fc0c17442': 1,
  '9eb2532ffa305903307527ed26561752ae2716a465c7347f6bb03d38987efb3d': 1,
  'ae51780dba21824a7515bfffef90bd4f78599ea6af3cc40ef36db98ec6dc93a4': 1,
  'b7236676fb5159713524fe49af418c62b87162e905dab02bef27a1b2badde5a3': 1,
  'bf606094d92ae7fcf6e97dd3f9f690ff00d2954558e1c92671c1c455c0535065': 1,
  'c5a021da6919a28b758460074e6af502c4c6d47cf4f00443bd987daccb600548': 2,
  'c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa': 3,
  'f9cbb2adfc98c4ca1e0d379b2b1c90b52a3e91fd61f8beaebb998423812ddf8d': 1},
 {'06eab8c4791cec49c676233d2e64048f1eaefe972dddfc52dfdffa06b4240730': 1,
  '08746e27d5c5b4fa82d1534a206c0f5932b85da2bc45429c8e9bd888544ebff4': 1,
  '2485f9b0677fc07b9dc4f3202a0dbfc7dd3b13e82476cd509be3282e8ce4d863': 1,
  '2b6c87682e5a05f8f7b1a34a56d5d415f322353ec9c26d679431d57de7aad64b': 2,
  '417cd86311e6261c7b1911896dc467829c4cfafd1fc48956cfbce153fbc17582': 1,
  '44284ddcfc94ad5da1cd7f5576dc01737ad7eeb0f75aa49c95bf42849d08790a': 1,
  '4d14eae20fff4dab766584ec10ba890cc51f49fbf86cf1e5d95860110212dbf1': 1,
  '520659ca128f10f1f3f1a724ed704b116b8f56da364dcd28fd6aaabfc25c5872': 1,
  '96134b6210eed5e21fe96c0412c596ffa47622ce39c4f6247960efe6dec683a2': 1,
  '97e495223a50ee0b0214ca3f811598e16b758b9bb3eb780db321e78fc0c17442': 1,
  '9eb2532ffa305903307527ed26561752ae2716a465c7347f6bb03d38987efb3d': 1,
  'ae51780dba21824a7515bfffef90bd4f78599ea6af3cc40ef36db98ec6dc93a4': 1,
  'b7236676fb5159713524fe49af418c62b87162e905dab02bef27a1b2badde5a3': 1,
  'bf606094d92ae7fcf6e97dd3f9f690ff00d2954558e1c92671c1c455c0535065': 1,
  'c5a021da6919a28b758460074e6af502c4c6d47cf4f00443bd987daccb600548': 1,
  'c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa': 2,
  'de4e8a9f01fa2526af01b897df7e71eb6422d5bda2faabb22527e73503e6a5a6': 1,
  'f9cbb2adfc98c4ca1e0d379b2b1c90b52a3e91fd61f8beaebb998423812ddf8d': 1})}

# Exact same-site snapshot pending-token re-expression, using the existing
# literal whole-map transition table under the standing rehash authorization.
HISTORY_BOOTSTRAPS['venues/paradex/src/account.rs'] = ({'0bc0f05583257432e512c356c6a5b41046e2a70d3cfe6eb8190238c9514faba2': 1,
  '3205be4fd57aee0c1cacd8fdd9fa48f9d000e9bbb2b7f53409ca39c64f0edc4a': 1,
  '388c9f66c40d488788f2f4a9808088e22a0425615dfd2818c9d63d58675392d1': 1,
  '395b8568f2efae46398fc76c6b422807900a928734749cdd4f8bd60b42366d19': 1,
  '5c28b0ebae1f2a54f344d52c2c422801c8e82381c7fd8ad95d13cd549e88ddec': 1,
  '85a7a04ae14d4df7bd123dd4837791701bbb02dc615d4e9f24060a2161f7ba3e': 1,
  'a5c107b3d98e73b4c73ebd13c21dcf8cb307a0041bc896cc3e6e808fb63cf446': 1,
  'ac42e25ca2de115b1eb59cc4913e6fd26670c4dfb47f17ed56fc0d1f34272fd9': 1,
  'c8154248a24f58ded15a175a8afd344f331635f51e564a526d510748797ff231': 1,
  'e473c92d9c3a15cbdf8752a673c64963c9ef531ee4a7e47ae60d1abe5e32659e': 1},
 {'0bc0f05583257432e512c356c6a5b41046e2a70d3cfe6eb8190238c9514faba2': 1,
  '29a6633fa3f6e05f04f534c0e675690014adde1e8a26e93c3769e8618602c13c': 1,
  '3205be4fd57aee0c1cacd8fdd9fa48f9d000e9bbb2b7f53409ca39c64f0edc4a': 1,
  '388c9f66c40d488788f2f4a9808088e22a0425615dfd2818c9d63d58675392d1': 1,
  '395b8568f2efae46398fc76c6b422807900a928734749cdd4f8bd60b42366d19': 1,
  '85a7a04ae14d4df7bd123dd4837791701bbb02dc615d4e9f24060a2161f7ba3e': 1,
  'a5c107b3d98e73b4c73ebd13c21dcf8cb307a0041bc896cc3e6e808fb63cf446': 1,
  'ac42e25ca2de115b1eb59cc4913e6fd26670c4dfb47f17ed56fc0d1f34272fd9': 1,
  'c8154248a24f58ded15a175a8afd344f331635f51e564a526d510748797ff231': 1,
  'e473c92d9c3a15cbdf8752a673c64963c9ef531ee4a7e47ae60d1abe5e32659e': 1})

# Owner-authorized exact two-site recipe-factory rehash. No additional calls.
SIGNER_POLICY_BOOTSTRAPS = {'venues/binance/src/history.rs': ({'153ef00b195a92e18337317fde397c0707f7778eadc4bfc02b2c0d2ffbf493c5': 1,
                                    '2263b7c3975168a800233a5720729a5293adc8d79ddc822742da439875b1c927': 1,
                                    '2b6c87682e5a05f8f7b1a34a56d5d415f322353ec9c26d679431d57de7aad64b': 3,
                                    '5d6691e305370ef1b5e22426e494d28b28936de80d5d5af079e9deae52eb33f2': 1,
                                    '6a3d37fddbb5fff8f3cd2c8d2ab4958d84b5c1efd268fd69c1464b41183a0c6a': 1,
                                    '7b98ff7fe9c2b0fba3fa2034b1d3cddb9008a47f9c7731d28364b7097fb1b66d': 1,
                                    '896c2dc203c518dc9ae1c6b5eb51b5591da28af19f9ff2461c4c629a490fb1a5': 1,
                                    'a2e0593410ad976880489badf3fbdf89ad9f79859b04b9860d23044f327967d3': 1,
                                    'abe91b4dc9b434e9eb3b21058d48e235a63990574ed4431b56d81aa1bb372f49': 1,
                                    'c5a021da6919a28b758460074e6af502c4c6d47cf4f00443bd987daccb600548': 2,
                                    'c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa': 2,
                                    'f859db20866f37469af70c624ec5ad2f31c4f7bd10c1bd1b0da41299b91f7eb7': 1},
                                   {'153ef00b195a92e18337317fde397c0707f7778eadc4bfc02b2c0d2ffbf493c5': 1,
                                    '2263b7c3975168a800233a5720729a5293adc8d79ddc822742da439875b1c927': 1,
                                    '2b6c87682e5a05f8f7b1a34a56d5d415f322353ec9c26d679431d57de7aad64b': 3,
                                    '5d6691e305370ef1b5e22426e494d28b28936de80d5d5af079e9deae52eb33f2': 1,
                                    '6a3d37fddbb5fff8f3cd2c8d2ab4958d84b5c1efd268fd69c1464b41183a0c6a': 1,
                                    '896c2dc203c518dc9ae1c6b5eb51b5591da28af19f9ff2461c4c629a490fb1a5': 1,
                                    '931fea11387a3eb1ccf2c909b869b3ee559925055e436a3987df14969b35fc81': 1,
                                    'a2e0593410ad976880489badf3fbdf89ad9f79859b04b9860d23044f327967d3': 1,
                                    'abe91b4dc9b434e9eb3b21058d48e235a63990574ed4431b56d81aa1bb372f49': 1,
                                    'c5a021da6919a28b758460074e6af502c4c6d47cf4f00443bd987daccb600548': 2,
                                    'c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa': 2,
                                    'f859db20866f37469af70c624ec5ad2f31c4f7bd10c1bd1b0da41299b91f7eb7': 1}),
 'venues/binance/src/lib.rs': ({'088924f3ac9b1699da1a2b0c712052ec5d14e133e51b4a93094827c12109ef7e': 1,
                                '29866cd39178c8da46ea6e7de031a9cd684e76f2734dbc6f60a081be3980972b': 1,
                                '5ed40af7dbe1e5b76ca8be90e29eabcbce35d0d0c768e765826c2b6525a06ada': 1,
                                '660826b694da348256b3fbbb3d2e4429c0be08068f931faa9b0cf92bae92004d': 1,
                                'c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa': 1},
                               {'088924f3ac9b1699da1a2b0c712052ec5d14e133e51b4a93094827c12109ef7e': 1,
                                '29866cd39178c8da46ea6e7de031a9cd684e76f2734dbc6f60a081be3980972b': 1,
                                '5ed40af7dbe1e5b76ca8be90e29eabcbce35d0d0c768e765826c2b6525a06ada': 1,
                                '931fea11387a3eb1ccf2c909b869b3ee559925055e436a3987df14969b35fc81': 1,
                                'c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa': 1})}

# Owner-approved temporary staging sites (6cf35cca, sign_http), exactly one per family:
# BingX legacy history and listen-key work, until cxxyqr's history bridge (#1848) and the
# session Context (#1832/#1842) own them; WEEX account cycle, trading and history, until
# the shared Plan/execution-group API and the history bridge own them. No other additions.
# OKX legacy GETs and sign failures (#1773): approved by claude-ddi2il, same exit as WEEX.
# Bitget private sends, signing and sign failures funnelled through lib.rs: approved by
# claude-ddi2il, same exit; remove before master.
STAGING_BOOTSTRAPS = {'venues/bingx/src/lib.rs': ({'1620031f97e957d61d90d1f02aeed59230d68b71da7aea4f13fe7ae663dcdc43': 1,
 '34b6cbe60cecfea179e31290b2eb916f11a7555598d45b795b4633e6afc768fc': 2,
 'b4b475ef7a3c08d3514bf2b7497dc3a85e7a328c33757287a83f82cb602f0a27': 1,
 'c5a021da6919a28b758460074e6af502c4c6d47cf4f00443bd987daccb600548': 1,
 'c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa': 2,
 'ca1a695870588abe078efa57b2d1e6f789b219044369ce2f8d1119a9c10664b7': 1,
 'fc8a5b682da002c9a8b697b45ebe204db49576e4cd13a275d46a52c7e13f01e6': 2},
    {'1620031f97e957d61d90d1f02aeed59230d68b71da7aea4f13fe7ae663dcdc43': 1,
 '34b6cbe60cecfea179e31290b2eb916f11a7555598d45b795b4633e6afc768fc': 1,
 '6cf35cca09778b64b2c0236671ce5549e5aa10a38a919d5d30317ddcfc43d219': 1,
 'b4b475ef7a3c08d3514bf2b7497dc3a85e7a328c33757287a83f82cb602f0a27': 1,
 'c5a021da6919a28b758460074e6af502c4c6d47cf4f00443bd987daccb600548': 1,
 'c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa': 2,
 'ca1a695870588abe078efa57b2d1e6f789b219044369ce2f8d1119a9c10664b7': 1,
 'fc8a5b682da002c9a8b697b45ebe204db49576e4cd13a275d46a52c7e13f01e6': 1}),
 'venues/bitget/src/lib.rs': ({'43cf91072f928d61f9f061f65a83f846ce5b37f7a7328105f8f2c390ba4ccf62': 1,
 '569abc443c07aca5127ee71c347600be68bb35c14aeae80c3a14b2f5f2642980': 1,
 '7280fdcd54919b50f6ab0d1bd012fb28e52c546b5b6c457f180576d9c4b2c35f': 1,
 'a5c4555a7341b5353adf967e17b11df1c12578e05ceda5525ccf44fddb177b4a': 1},
    {'43cf91072f928d61f9f061f65a83f846ce5b37f7a7328105f8f2c390ba4ccf62': 1,
 '569abc443c07aca5127ee71c347600be68bb35c14aeae80c3a14b2f5f2642980': 1,
 '5b44116449d06833b5f091cb3866ced7f16f181fd1226b31ad977eff6bf26ac9': 1,
 '6c77d608b079104cc5338cbabb01a7c4a0e24f112ef91ea5483c423808d92bad': 1,
 '7280fdcd54919b50f6ab0d1bd012fb28e52c546b5b6c457f180576d9c4b2c35f': 1,
 'a5c4555a7341b5353adf967e17b11df1c12578e05ceda5525ccf44fddb177b4a': 1}),
 'venues/okx/src/lib.rs': ({'1cb580e20171923e6928e9cbc28b73b880ba2dd8c4f11a395557966119b8d038': 1,
 '2b6c87682e5a05f8f7b1a34a56d5d415f322353ec9c26d679431d57de7aad64b': 2,
 'a39517b8d72cf193e89578682d47a093f99c8b47724978190a59c88125595faf': 2,
 'b8eef2a79ada04918fd91d4c45f3a03a76d566ac79f733acf0c441b0fbbfc934': 1,
 'c5a021da6919a28b758460074e6af502c4c6d47cf4f00443bd987daccb600548': 1,
 'c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa': 1},
    {'1cb580e20171923e6928e9cbc28b73b880ba2dd8c4f11a395557966119b8d038': 1,
 '2b6c87682e5a05f8f7b1a34a56d5d415f322353ec9c26d679431d57de7aad64b': 2,
 '2e9218906fd923fa6c83c648873ea2527c33dd1068d609c8c1ec343630b36911': 1,
 'a39517b8d72cf193e89578682d47a093f99c8b47724978190a59c88125595faf': 2,
 'b1761cdefa174ea70037cfceab33b91ad7279e59d22ccecb872e996dd5b0c32c': 1,
 'b8eef2a79ada04918fd91d4c45f3a03a76d566ac79f733acf0c441b0fbbfc934': 1,
 'c5a021da6919a28b758460074e6af502c4c6d47cf4f00443bd987daccb600548': 1,
 'c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa': 2}),
 'venues/weex/src/lib.rs': ({'067aa233d0ebbb24c837ae5cb4549d6aa68e9f29bf4bdde0ba0630bda9f466c1': 1,
 '0d6bbb8e01da4f9bc9a177c0e8ca9d0d015c922780050cfeda6b33807cac8e78': 1,
 '1620031f97e957d61d90d1f02aeed59230d68b71da7aea4f13fe7ae663dcdc43': 1,
 '1805af84e51e32d19a267749aeea1b85d4e801cb4df04d796274edac35db929d': 1,
 '34b6cbe60cecfea179e31290b2eb916f11a7555598d45b795b4633e6afc768fc': 1,
 '43cf91072f928d61f9f061f65a83f846ce5b37f7a7328105f8f2c390ba4ccf62': 1,
 '497d185372b077e0511e2f21fa2b9b14522eab0cd1c71ac0f7a65741155d0a23': 1,
 '61034a5650efd3373a0ab46c4948145e4c4022d60a9c7f0898563e273d100159': 1,
 '6f0b47296411f75eb3a85b182bf43c08378b387a1a584d8848b0e811f29b01f9': 1,
 '869f03854972431807651e3f560b5dbb7232088a7fcc89705e7a58f747bb3e2a': 1,
 '93e77dda58930ad87b59e2334bedbf6fbbef499f277e47697e979bc8e474f9e3': 1,
 'a40239e5961362c920c8e84905898477b16673e64c7d94eacaa5b51ba63a1152': 1,
 'ab3206b428125b95aea7df47c71c9bfb8cef1809ff9c4432ba4ab8f34db5bac9': 1,
 'c5a021da6919a28b758460074e6af502c4c6d47cf4f00443bd987daccb600548': 1,
 'c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa': 3,
 'cd66da21afeeef22b98026c1d44238cf77b537c7b5ea67ee5d4fb4464905d893': 1,
 'fc8a5b682da002c9a8b697b45ebe204db49576e4cd13a275d46a52c7e13f01e6': 1},
    {'067aa233d0ebbb24c837ae5cb4549d6aa68e9f29bf4bdde0ba0630bda9f466c1': 1,
 '0d6bbb8e01da4f9bc9a177c0e8ca9d0d015c922780050cfeda6b33807cac8e78': 1,
 '1620031f97e957d61d90d1f02aeed59230d68b71da7aea4f13fe7ae663dcdc43': 1,
 '1805af84e51e32d19a267749aeea1b85d4e801cb4df04d796274edac35db929d': 1,
 '34b6cbe60cecfea179e31290b2eb916f11a7555598d45b795b4633e6afc768fc': 1,
 '43cf91072f928d61f9f061f65a83f846ce5b37f7a7328105f8f2c390ba4ccf62': 1,
 '61034a5650efd3373a0ab46c4948145e4c4022d60a9c7f0898563e273d100159': 1,
 '6cf35cca09778b64b2c0236671ce5549e5aa10a38a919d5d30317ddcfc43d219': 1,
 '6f0b47296411f75eb3a85b182bf43c08378b387a1a584d8848b0e811f29b01f9': 1,
 '869f03854972431807651e3f560b5dbb7232088a7fcc89705e7a58f747bb3e2a': 1,
 '93e77dda58930ad87b59e2334bedbf6fbbef499f277e47697e979bc8e474f9e3': 1,
 'a40239e5961362c920c8e84905898477b16673e64c7d94eacaa5b51ba63a1152': 1,
 'ab3206b428125b95aea7df47c71c9bfb8cef1809ff9c4432ba4ab8f34db5bac9': 1,
 'c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa': 2,
 'cd66da21afeeef22b98026c1d44238cf77b537c7b5ea67ee5d4fb4464905d893': 1,
 'fc8a5b682da002c9a8b697b45ebe204db49576e4cd13a275d46a52c7e13f01e6': 1})}



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
            if path in SIGNER_POLICY_BOOTSTRAPS and bound == SIGNER_POLICY_BOOTSTRAPS[path][0]:
                bound = SIGNER_POLICY_BOOTSTRAPS[path][1]
            if path in STAGING_BOOTSTRAPS and bound == STAGING_BOOTSTRAPS[path][0]:
                bound = STAGING_BOOTSTRAPS[path][1]
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
