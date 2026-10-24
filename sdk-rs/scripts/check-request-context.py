#!/usr/bin/env python3
"""Exact, shrinking inventory of opaque venue HTTP/completion provenance.

The request inventory freezes legacy calls whose receiver lexically resolves to a
`Cx` parameter, refusing aliases it cannot follow; #[cfg(test)] sites are listed
separately. `fingerprints`/`sites` stay the plain lexical scan the socket
inventory reuses. Neither confers managed retry, release or outcome guarantees.
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
TEST_MANIFEST = 'scripts/request-legacy-test-sites.json'
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


class InventoryRefusal(ValueError):
    """A shape the inventory cannot resolve; the message never quotes source."""


CX = 'Cx'
FN_SIGNATURES = {'Fn', 'FnMut', 'FnOnce', 'fn'}
CLOSURE_OPENERS = {'(', ',', '=', '{', ';', 'move', 'return', '[', '!'}
CALL_PREFIX_KEYWORDS = {'if', 'match', 'while', 'return', 'in', 'let', 'for', 'break', 'else', 'move', 'as'}
TEST_ATTRIBUTE = ['#', '[', 'cfg', '(', 'test', ')', ']']


def split_top(segment):
    """Split a parameter list at depth-0 commas (angles included)."""
    parts, current, depth = [], [], 0
    for index, token in enumerate(segment):
        if token in '([{':
            depth += 1
        elif token in ')]}':
            depth -= 1
        elif token == '<':
            depth += 1
        elif token == '>' and not (index and segment[index - 1] == '-'):
            depth -= 1
        if token == ',' and depth == 0:
            parts.append(current)
            current = []
        else:
            current.append(token)
    if current:
        parts.append(current)
    return parts


def cx_type(segment, offset):
    """Index of the top-level Cx in `&'a mut path::Cx<..>`, else None."""
    k = 0
    while k < len(segment) and segment[k] in {'&', 'mut', "'"}:
        k += 2 if segment[k] == "'" else 1
    last = None
    while k < len(segment) and IDENT.fullmatch(segment[k]):
        last = k
        k += 1
        if segment[k:k + 1] != ['::']:
            break
        k += 1
    if last is not None and segment[last] == CX and (k == len(segment) or segment[k] == '<') \
            and (k == len(segment) or group_end(segment, k) == len(segment)):
        return offset + last
    return None


def inferred(segment):
    """`_`, `&_` or `&'a mut _`: the compiler, not the text, chooses the type."""
    rest = [token for k, token in enumerate(segment)
            if token not in {'&', 'mut', "'"} and not (k and segment[k - 1] == "'")]
    return rest == ['_']


def parameter(part):
    if ':' in part:
        colon = part.index(':')
        pattern, kind = part[:colon], part[colon + 1:]
    else:
        colon, pattern, kind = None, part, None
    pattern = [token for token in pattern if token != 'mut']
    name = pattern[0] if len(pattern) == 1 and IDENT.fullmatch(pattern[0]) and pattern[0] not in {'self', '_'} else None
    return name, colon, kind


def expression_end(stream, index):
    while index < len(stream):
        if stream[index] in {'(', '[', '{'}:
            index = group_end(stream, index)
        elif stream[index] in {')', ']', '}', ',', ';'}:
            return index
        else:
            index += 1
    return index


def closure_bar(stream, index):
    return (stream[index] == '|' and index and stream[index + 1:index + 2] != ['|']
            and (stream[index - 1] in CLOSURE_OPENERS or stream[index - 2:index] == ['=', '>']))


def bindings(stream):
    """Lexically scoped parameters: (name, kind, scope start, scope end)."""
    found, declarations, allowed_types = [], set(), set()

    def bind(parts, start, end, base, untyped):
        offset = base
        for part in parts:
            name, colon, kind = parameter(part)
            if colon is not None:
                position = cx_type(kind, offset + colon + 1)
                if position is not None:
                    allowed_types.add(position)
            if name:
                declarations.add(offset + part.index(name))
                if kind is None or inferred(kind):
                    kind_name = 'closure' if untyped else 'other'
                else:
                    kind_name = 'cx' if cx_type(kind, 0) is not None else 'other'
                found.append((name, kind_name, start, end))
            offset += len(part) + 1

    for i, token in enumerate(stream):
        if token == 'fn' and i + 2 < len(stream) and IDENT.fullmatch(stream[i + 1]):
            j = i + 2
            if stream[j] == '<':
                j = group_end(stream, j)
            if stream[j] != '(':
                continue
            params_end = group_end(stream, j)
            k = params_end
            while k < len(stream) and stream[k] not in {'{', ';'}:
                k = group_end(stream, k) if stream[k] in {'(', '['} else k + 1
            end = group_end(stream, k) if k < len(stream) and stream[k] == '{' else k
            bind(split_top(stream[j + 1:params_end - 1]), k, end, j + 1, False)
        elif token == '(' and i and stream[i - 1] in FN_SIGNATURES:
            # Fn/FnMut/FnOnce/fn signatures name a Cx without binding it.
            offset = i + 1
            for part in split_top(stream[i + 1:group_end(stream, i) - 1]):
                position = cx_type(part, offset)
                if position is not None:
                    allowed_types.add(position)
                offset += len(part) + 1
        elif closure_bar(stream, i):
            j = i + 1
            while j < len(stream) and stream[j] != '|':
                j = group_end(stream, j) if stream[j] in {'(', '['} else j + 1
            if j + 1 >= len(stream):
                raise InventoryRefusal('unbalanced closure parameters')
            k = j + 1
            if stream[k] == '-':
                while k < len(stream) and stream[k] != '{':
                    k += 1
            end = group_end(stream, k) if stream[k] == '{' else expression_end(stream, k)
            bind(split_top(stream[i + 1:j]), k, end, i + 1, True)
    return found, declarations, allowed_types


PATTERN_WORDS = {'mut', 'ref', 'let', 'if', 'while', 'for', 'in', 'match', 'box', 'self', 'true', 'false'}
FLOW = {'cx', 'closure', 'derived'}


def value_use(stream, found, index):
    """A Cx-flow binding used as a value rather than through a member."""
    token = stream[index]
    return (IDENT.fullmatch(token) is not None and not (index and stream[index - 1] in {'.', '::'})
            and stream[index + 1:index + 2] != ['.'] and resolve(found, token, index) in FLOW)


def pattern_names(stream, start, end):
    return [k for k in range(start, end)
            if IDENT.fullmatch(stream[k]) and stream[k] not in PATTERN_WORDS and stream[k][0].islower()
            and stream[k + 1:k + 2] not in [['('], ['{'], ['::'], ['!']] and stream[k - 1:k] != ['::']]


def enclosing_end(stream, index):
    depth = 0
    for k in range(index, len(stream)):
        if stream[k] in {'(', '[', '{'}:
            depth += 1
        elif stream[k] in {')', ']', '}'}:
            if depth == 0:
                return k
            depth -= 1
    return len(stream)


def derived_bindings(stream, found):
    """Untyped let/for/match bindings fed by a Cx value: conservatively Cx-bound."""
    for i, token in enumerate(stream):
        if token == 'let':
            equals = i + 1
            while equals < len(stream) and stream[equals] not in {'=', ';'}:
                equals = group_end(stream, equals) if stream[equals] in {'(', '[', '{'} else equals + 1
            if equals >= len(stream) or stream[equals] != '=':
                continue
            colon = next((k for k in range(i + 1, equals) if stream[k] == ':'), None)
            if colon is not None and not inferred(stream[colon + 1:equals]):
                continue
            pattern = (i + 1, colon if colon is not None else equals)
            if stream[i - 1:i] in [['if'], ['while']]:
                initializer_end = equals + 1
                while initializer_end < len(stream) and stream[initializer_end] != '{':
                    initializer_end = (group_end(stream, initializer_end)
                                       if stream[initializer_end] in {'(', '['} else initializer_end + 1)
                if initializer_end >= len(stream):
                    continue
                scope = (initializer_end, group_end(stream, initializer_end))
            else:
                initializer_end = expression_end(stream, equals + 1)
                scope = (initializer_end, enclosing_end(stream, initializer_end))
        elif token == 'for':
            within = i + 1
            while within < len(stream) and stream[within] != 'in':
                within += 1
            if within >= len(stream):
                continue
            body = within + 1
            while body < len(stream) and stream[body] != '{':
                body = group_end(stream, body) if stream[body] in {'(', '['} else body + 1
            if body >= len(stream):
                continue
            pattern, equals, initializer_end = (i + 1, within), within, body
            scope = (body, group_end(stream, body))
        elif token == 'match':
            body = i + 1
            while body < len(stream) and stream[body] != '{':
                body = group_end(stream, body) if stream[body] in {'(', '['} else body + 1
            if body >= len(stream):
                continue
            if any(value_use(stream, found, k) for k in range(i + 1, body)):
                end = group_end(stream, body)
                for k in pattern_names(stream, body + 1, end - 1):
                    found.append((stream[k], 'derived', k, end))
            continue
        else:
            continue
        if any(value_use(stream, found, k) for k in range(equals + 1, initializer_end)):
            for k in pattern_names(stream, *pattern):
                found.append((stream[k], 'derived', *scope))


def resolve(found, name, index):
    best = None
    for bound, kind, start, end in found:
        if bound == name and start <= index < end and (best is None or end - start < best[1]):
            best = (kind, end - start)
    return best and best[0]


def call_argument(stream, index):
    """True when stream[index] (after & / mut / * prefixes) is a whole call argument."""
    before = index - 1
    while before >= 0 and stream[before] in {'&', 'mut', '*'}:
        before -= 1
    if before < 0 or stream[before] not in {'(', ','} or stream[index + 1:index + 2] not in [[','], [')']]:
        return False
    depth, opening = 0, before
    while opening >= 0:
        token = stream[opening]
        if token in {')', ']', '}'}:
            depth += 1
        elif token in {'(', '[', '{'}:
            if depth == 0:
                break
            depth -= 1
        opening -= 1
    if opening <= 0 or stream[opening] != '(':
        return False
    callee = stream[opening - 1]
    return (callee in {'>', ')', ']'} or bool(IDENT.fullmatch(callee))) and callee not in CALL_PREFIX_KEYWORDS


def check_cx_names(stream, allowed_types):
    uses = [(i, i + stream[i:].index(';')) for i, token in enumerate(stream) if token == 'use']
    for i, token in enumerate(stream):
        if token != CX or i in allowed_types:
            continue
        if any(start < i < end for start, end in uses) and stream[i + 1:i + 2] != ['as']:
            continue
        if stream[i + 1:i + 2] == ['::'] or (stream[i - 1:i] == ['<'] and stream[i + 1:i + 2] == ['as']):
            continue
        raise InventoryRefusal('Cx type outside a parameter, import or Fn signature')


def check_cx_values(stream, found, declarations):
    macros = [(i + 2, group_end(stream, i + 2)) for i in range(len(stream))
              if stream[i + 1:i + 2] == ['!'] and stream[i + 2:i + 3] in [['('], ['{'], ['[']]
              and stream[i] != 'macro_rules']
    for i, token in enumerate(stream):
        if not IDENT.fullmatch(token) or i in declarations or (i and stream[i - 1] in {'.', '::'}):
            continue
        kind = resolve(found, token, i)
        if kind == 'cx':
            member = stream[i + 1:i + 2] == ['.'] and stream[i + 2:i + 3] != ['.']
            if member or call_argument(stream, i):
                continue
            # A macro's own argument is a token tree it may rebind or splice.
            if any(start < i < end for start, end in macros):
                raise InventoryRefusal('Cx value inside a macro')
            raise InventoryRefusal('Cx alias or rebinding')


def test_ranges(stream):
    ranges = []
    for i in range(len(stream)):
        if stream[i:i + len(TEST_ATTRIBUTE)] != TEST_ATTRIBUTE:
            continue
        k = i + len(TEST_ATTRIBUTE)
        while stream[k:k + 1] == ['#']:
            k = group_end(stream, k + 1)
        while k < len(stream):
            token = stream[k]
            if token == ';':
                k += 1
                break
            if token in {',', ')', ']', '}'}:
                break
            if token == '{':
                k = group_end(stream, k)
                if stream[k:k + 1] == ['else']:
                    k += 1
                    continue
                break
            k = group_end(stream, k) if token in {'(', '['} else k + 1
        ranges.append((i, k))
    return ranges


def context_fingerprints(source):
    """(prod, test) fingerprints of legacy calls on a resolved Cx binding."""
    stream = tokens(source)
    definitions = []
    for i, token in enumerate(stream):
        if token == 'macro_rules' and stream[i + 1:i + 2] == ['!']:
            opening = i + 3
            if opening >= len(stream) or stream[opening] not in {'(', '[', '{'}:
                raise ValueError('invalid macro definition')
            definitions.append((i, group_end(stream, opening)))
    found, declarations, allowed_types = bindings(stream)
    check_cx_names(stream, allowed_types)
    check_cx_values(stream, found, declarations)
    derived_bindings(stream, found)
    tests = test_ranges(stream)
    prod, test = collections.Counter(), collections.Counter()
    counted = []

    def add(start, end, at):
        payload = json.dumps(stream[start:end], ensure_ascii=False, separators=(',', ':'))
        target = test if any(a <= at < b for a, b in tests) else prod
        target[hashlib.sha256(payload.encode()).hexdigest()] += 1

    for i, token in enumerate(stream):
        name = token.removeprefix('r#')
        member = i and stream[i - 1] in {'.', '::'} and (i < 2 or stream[i - 2] != '.')
        indirect = token == '$' and member
        if (name in METHODS and member) or indirect:
            if any(start <= i < end for start, end in definitions):
                raise ValueError('legacy provenance under macro definition')
            end = i + (2 if indirect else 1)
            if stream[end:end + 2] == ['::', '<']:
                end = group_end(stream, end + 1)
            call = stream[end:end + 1] == ['(']
            if call:
                end = group_end(stream, end)
            start = receiver_start(stream, i)
            if stream[i - 1] == '.':
                # A call, index, macro or field result over a Cx value counts too.
                legacy = call and ((start == i - 2 and resolve(found, stream[i - 2], i) in FLOW) or any(
                    value_use(stream, found, k) for k in range(start, i - 1)))
            else:
                path = stream[start:i]
                arguments = range(i + 1, end) if call else range(0)
                legacy = CX in path or any(value_use(stream, found, k) for k in arguments)
            if legacy:
                counted.append(i)
                add(start, end, i)
        if stream[i + 1:i + 2] == ['!'] and stream[i + 2:i + 3] in [['('], ['{'], ['[']]:
            end = group_end(stream, i + 2)
            if name == 'include':
                raise ValueError('unscanned Rust include')
    # A macro invocation is frozen only when it wraps a counted legacy call.
    for i, token in enumerate(stream):
        if stream[i + 1:i + 2] == ['!'] and stream[i + 2:i + 3] in [['('], ['{'], ['[']] and token != 'macro_rules':
            end = group_end(stream, i + 2)
            if any(i + 3 <= at < end for at in counted):
                add(i, end, i)
    return dict(prod), dict(test)


def module_declarations(stream):
    """[(segments, path attribute, test)] for every out-of-line `mod x;`."""
    tests = test_ranges(stream)
    inline = [(stream[i + 1], i, group_end(stream, i + 2)) for i, token in enumerate(stream)
              if token == 'mod' and i + 2 < len(stream) and stream[i + 2] == '{']
    found = []
    for i, token in enumerate(stream):
        if token == 'mod' and i + 2 < len(stream) and stream[i + 2] == ';' and IDENT.fullmatch(stream[i + 1]):
            segments = [name for name, start, end in sorted(inline, key=lambda r: r[1]) if start < i < end]
            attribute = None
            k = i - 1
            while k >= 0 and stream[k] in {'pub', ')', 'crate', 'super', '('}:
                k -= 1
            while k >= 0 and stream[k] == ']':
                depth, opening = 0, k
                while opening >= 0:
                    depth += (stream[opening] == ']') - (stream[opening] == '[')
                    if depth == 0:
                        break
                    opening -= 1
                if stream[opening + 1:opening + 3] == ['path', '='] and attribute is None:
                    attribute = json.loads(stream[opening + 3])
                k = opening - 2
            found.append((segments + [stream[i + 1]], attribute, any(a <= i < b for a, b in tests)))
    return found


def request_sites():
    sources = {path.resolve(): path.read_text() for path in sorted((ROOT / 'venues').glob('*/src/**/*.rs'))}
    root = ROOT.resolve()
    children = {}
    for path, source in sources.items():
        resolved = []
        for segments, attribute, is_test in module_declarations(tokens(source)):
            if attribute is not None:
                candidates = [path.parent / attribute]
            else:
                base = path.parent if path.name in {'lib.rs', 'main.rs', 'mod.rs'} else path.parent / path.stem
                folder = base.joinpath(*segments[:-1])
                candidates = [folder / f'{segments[-1]}.rs', folder / segments[-1] / 'mod.rs']
            resolved += [(candidate.resolve(), is_test) for candidate in candidates if candidate.resolve() in sources]
        children[path] = resolved
    def reachable(starts, follow_test):
        seen, pending = set(), list(starts)
        while pending:
            current = pending.pop()
            if current not in seen:
                seen.add(current)
                pending += [child for child, is_test in children.get(current, []) if follow_test or not is_test]
        return seen

    # A file any prod declaration reaches stays prod; only test-only files move.
    roots = [path for path in sources if path.name in {'lib.rs', 'main.rs'} and path.parent.name == 'src']
    test_files = reachable([child for links in children.values() for child, is_test in links if is_test],
                           True) - reachable(roots, False)
    prod, test = {}, {}
    for path, source in sources.items():
        relative = str(path.relative_to(root))
        try:
            found_prod, found_test = context_fingerprints(source)
        except InventoryRefusal as refusal:
            raise InventoryRefusal(f'{relative}: {refusal}') from None
        if path in test_files:
            found_test = dict(collections.Counter(found_prod) + collections.Counter(found_test))
            found_prod = {}
        if found_prod:
            prod[relative] = found_prod
        if found_test:
            test[relative] = found_test
    return prod, test


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


def subset(counts, baseline):
    return all(count <= baseline.get(digest, 0) for digest, count in counts.items())


def check(current, allowed, previous):
    failures = []
    if current != allowed:
        failures.append('current sites must exactly match the inventory (prune removals; no new sites)')
    for label, baseline in previous:
        for path, counts in allowed.items():
            if not subset(counts, baseline.get(path, {})):
                failures.append(f'{path}: inventory grew against {label}')
    return failures


def git(*args):
    return subprocess.run(['git', '-C', str(ROOT), *args], text=True, capture_output=True, check=True).stdout


def previous_manifests(inventory=None, predecessor=None):
    """Per-ref baselines; a ref without `inventory` is bounded by `predecessor`."""
    inventory = inventory or MANIFEST
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
    previous = []
    for ref in refs:
        # A missing ref is not a missing manifest. CI must fetch history first.
        git('rev-parse', '--verify', f'{ref}^{{commit}}')
        for name in [inventory, predecessor]:
            path = git('rev-parse', '--show-prefix').strip() + name if name else None
            if path and path in git('ls-tree', '--full-name', '--name-only', ref, '--', name).splitlines():
                previous.append((ref, manifest(git('show', f'{ref}:{path}'))))
                break
    return previous


def main():
    try:
        prod, test = request_sites()
        failures = check(prod, manifest((ROOT / MANIFEST).read_text()), previous_manifests())
        failures += ['test inventory: ' + failure for failure in check(
            test, manifest((ROOT / TEST_MANIFEST).read_text()), previous_manifests(TEST_MANIFEST, MANIFEST))]
    except InventoryRefusal as refusal:
        failures = [f'cannot verify legacy inventory: {refusal}']
    except (ValueError, OSError, KeyError, TypeError, subprocess.CalledProcessError) as error:
        # Never print source, literal contents or subprocess stdout/stderr.
        failures = [f'cannot verify legacy inventory: {type(error).__name__}']
    for failure in failures:
        print('BANNED: request provenance: ' + failure)
    return bool(failures)


if __name__ == '__main__':
    raise SystemExit(main())
