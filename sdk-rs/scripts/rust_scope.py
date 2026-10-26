"""Conservative lexical name resolution for the inventory checkers.

`resolve` answers whether a bare type name at one token resolves to one exact
engine path. Brace blocks are scopes. The innermost enclosing scope that names
the type decides: an explicit unaliased import of the engine path is
'provider'; any other import, alias, item binder or generic parameter is
'other'; a scope reached only by globs is 'unknown'. An explicit import beats a
glob in the same scope, as in Rust. Anything unparsed stays conservative.
"""

import collections
import functools
import importlib.util
import pathlib
import re

ITEMS = {'struct', 'enum', 'type', 'trait', 'union', 'mod', 'fn', 'const', 'static'}
IDENT = re.compile(r'[A-Za-z_][A-Za-z_0-9]*')
_spec = importlib.util.spec_from_file_location(
    'request_tokens', pathlib.Path(__file__).with_name('check-request-context.py'))
_request = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_request)
request_tokens = _request.tokens
GENERIC_OWNERS = {'fn', 'struct', 'enum', 'impl', 'trait', 'type', 'union'}


def scopes(stream):
    """For every token index, the index of its innermost enclosing `{` (-1 at file scope)."""
    owner, stack = [], []
    for i, token in enumerate(stream):
        owner.append(stack[-1] if stack else -1)
        if token == '{':
            stack.append(i)
        elif token == '}' and stack:
            stack.pop()
    return owner


def block_end(stream, start):
    """Index just past the `}` matching the `{` at `start`."""
    depth = 0
    for i in range(start, len(stream)):
        depth += (stream[i] == '{') - (stream[i] == '}')
        if not depth:
            return i + 1
    raise ValueError('unbalanced block')


def use_leaves(stream, start):
    """(path, name, aliased, glob) for every leaf of the `use` at `start`; None if unparsed."""
    end = stream.index(';', start)
    leaves = []

    def walk(i, prefix):
        path = list(prefix)
        while i < end:
            token = stream[i]
            if token == '::':
                i += 1
            elif token == '*':
                leaves.append((path, '*', False, True))
                return i + 1
            elif token == '{':
                i += 1
                while stream[i] != '}':
                    i = walk(i, path)
                    if stream[i] == ',':
                        i += 1
                return i + 1
            elif token in {',', '}'}:
                return i
            elif stream[i + 1:i + 2] == ['as']:
                leaves.append((path + [token], stream[i + 2], True, False))
                return i + 3
            elif stream[i + 1] in {',', '}', ';'}:
                leaves.append((path + [token], token, False, False))
                return i + 1
            else:
                path.append(token)
                i += 1
        return i

    try:
        walk(start + 1, [])
    except (IndexError, ValueError):
        return None
    return leaves


def generic_binders(stream, name):
    """Token spans of items that declare `name` as a generic parameter."""
    spans = []
    for i, token in enumerate(stream):
        if token not in GENERIC_OWNERS:
            continue
        j = i + 1 + (token != 'impl')
        if stream[j:j + 1] != ['<']:
            continue
        depth, k, params = 0, j, []
        while k < len(stream):
            depth += (stream[k] == '<') - (stream[k] == '>')
            if depth == 1 and stream[k - 1] in {'<', ','} and stream[k] == name:
                params.append(k)
            k += 1
            if not depth:
                break
        if params:
            body = next((m for m in range(k, len(stream)) if stream[m] in {'{', ';'}), None)
            end = block_end(stream, body) if body is not None and stream[body] == '{' else k
            spans.append((i, end))
    return spans


def resolve(stream, name, index, provider_path):
    """'provider', 'other' or 'unknown' for the bare `name` used at `index`."""
    owner = scopes(stream)
    if any(start <= index < end for start, end in generic_binders(stream, name)):
        return 'other'
    explicit, foreign, globbed = set(), set(), set()
    for i, token in enumerate(stream):
        if token == 'use':
            leaves = use_leaves(stream, i)
            if leaves is None:
                foreign.add(owner[i])
                continue
            for path, leaf, aliased, glob in leaves:
                if glob:
                    globbed.add(owner[i])
                elif leaf == name:
                    good = not aliased and path == provider_path
                    (explicit if good else foreign).add(owner[i])
        elif token in ITEMS and stream[i + 1:i + 2] == [name]:
            foreign.add(owner[i])
    scope = owner[index]
    while True:
        if scope in foreign:
            return 'other'
        if scope in explicit:
            return 'provider'
        if scope in globbed:
            return 'unknown'
        if scope == -1:
            return 'unknown'
        scope = owner[scope]


# Crate-wide proofs. Lexical scope alone cannot see a name a macro, attribute
# or glob could introduce, so every such generator in a crate must be from a
# closed trusted set, every glob must be proven, and watched names must have no
# binder anywhere in the crate. Anything else refuses; callers keep counting.
TRUSTED_MACROS = {'format', 'matches', 'vec', 'assert', 'assert_eq', 'assert_ne', 'debug_assert',
                  'debug_assert_eq', 'debug_assert_ne', 'panic', 'unreachable', 'todo',
                  'unimplemented', 'write', 'writeln',
                  # Binds only the `static NAME` written in its body, which `binders` sees.
                  'thread_local'}
# Built-in attributes, plus the inert `serde`/`default` derive helpers.
TRUSTED_ATTRIBUTES = {'derive', 'cfg', 'allow', 'warn', 'deny', 'forbid', 'expect', 'inline',
                      'must_use', 'doc', 'path', 'test', 'repr', 'non_exhaustive', 'track_caller',
                      'cold', 'default', 'serde'}
STD_DERIVES = {'Debug', 'Clone', 'Copy', 'PartialEq', 'Eq', 'PartialOrd', 'Ord', 'Hash', 'Default'}
SERDE_DERIVES = {'Serialize', 'Deserialize'}
KEYWORDS = {'if', 'while', 'match', 'return', 'in', 'let', 'else', 'for', 'loop', 'move', 'as',
            'break', 'continue', 'mut', 'ref', 'where', 'unsafe', 'async', 'await', 'dyn', 'impl'}
WORKSPACE_MACRO_CRATES = {'safe_cex_crypto': 'crypto'}


def group_close(stream, start):
    """Index just past the group opened at `start` by `(`, `[` or `{`."""
    pairs = {'(': ')', '[': ']', '{': '}'}
    depth = 0
    for i in range(start, len(stream)):
        if stream[i] in pairs:
            depth += 1
        elif stream[i] in pairs.values():
            depth -= 1
            if not depth:
                return i + 1
    raise ValueError('unbalanced group')


def enum_variants(stream, at):
    """Variant names of the `enum` whose keyword is at `at`."""
    body = next(k for k in range(at, len(stream)) if stream[k] == '{')
    end, names, depth = group_close(stream, body), [], 0
    for k in range(body + 1, end - 1):
        if stream[k] in {'(', '[', '{'}:
            depth += 1
        elif stream[k] in {')', ']', '}'}:
            depth -= 1
        elif depth == 0 and stream[k - 1] in {'{', ',', ']'} and IDENT.fullmatch(stream[k]):
            names.append(stream[k])
    return names


def binders(stream):
    """(name, kind, detail) for everything in `stream` that can bind a type-namespace name."""
    found = []
    for i, token in enumerate(stream):
        following = stream[i + 1] if i + 1 < len(stream) else None
        if token in ITEMS and following and IDENT.fullmatch(following.removeprefix('r#')):
            found.append((following.removeprefix('r#'), token, i))
            if token == 'enum':
                found += [(name, 'variant', i) for name in enum_variants(stream, i)]
        elif token == 'macro_rules' and following == '!' and i + 2 < len(stream):
            found.append((stream[i + 2].removeprefix('r#'), 'macro_rules', i))
        elif token == 'macro' and following and IDENT.fullmatch(following):
            found.append((following, 'macro', i))
        elif stream[i:i + 2] == ['extern', 'crate']:
            end = stream.index(';', i)
            found.append((stream[end - 1].removeprefix('r#'), 'extern crate', i))
        elif token == 'use':
            leaves = use_leaves(stream, i)
            if leaves is None:
                found.append(('*', 'unparsed use', i))
                continue
            for path, leaf, aliased, glob in leaves:
                if not glob:
                    found.append((leaf.removeprefix('r#'), 'use', tuple(path) + (('as',) if aliased else ())))
    for i, token in enumerate(stream):
        if token not in GENERIC_OWNERS:
            continue
        j = i + 1 + (token != 'impl')
        if stream[j:j + 1] != ['<']:
            continue
        depth, k = 0, j
        while k < len(stream):
            depth += (stream[k] == '<') - (stream[k] == '>')
            if depth == 1 and stream[k - 1] in {'<', ','} and IDENT.fullmatch(stream[k]):
                found.append((stream[k], 'generic', i))
            k += 1
            if not depth:
                break
    return found


def macro_invocations(stream):
    """(path, index) of item- or statement-position macro invocations."""
    found = []
    for i, token in enumerate(stream):
        if stream[i + 1:i + 2] != ['!'] or stream[i + 2:i + 3] not in (['('], ['['], ['{']):
            continue
        if token in KEYWORDS or not IDENT.fullmatch(token.removeprefix('r#')):
            continue
        start = i
        while start >= 2 and stream[start - 1] == '::' and IDENT.fullmatch(stream[start - 2]):
            start -= 2
        if start and stream[start - 1] == '::':
            start -= 1
        if not start or stream[start - 1] in {';', '{', '}', ']'}:
            found.append(([t for t in stream[start:i + 1] if t != '::'], i))
    return found


def attributes(stream):
    """(path tokens, argument tokens) of every `#[..]` and `#![..]` attribute."""
    found = []
    for i, token in enumerate(stream):
        if token != '#':
            continue
        k = i + 1 + (stream[i + 1:i + 2] == ['!'])
        if stream[k:k + 1] != ['[']:
            continue
        end = group_close(stream, k)
        body = stream[k + 1:end - 1]
        path_end = 0
        while path_end < len(body) and (IDENT.fullmatch(body[path_end]) or body[path_end] == '::'):
            path_end += 1
        found.append((body[:path_end], body[path_end:]))
    return found


@functools.lru_cache(maxsize=None)
def impl_only_macro(sdk_root, crate, name):
    """Whether every `#[macro_export] macro_rules! name` in a workspace crate expands
    only to `impl` blocks, which can bind no name."""
    directory = WORKSPACE_MACRO_CRATES.get(crate)
    if directory is None:
        return False
    found = False
    for path in sorted((sdk_root / directory / 'src').glob('**/*.rs')):
        stream = request_tokens(path.read_text())
        for i, token in enumerate(stream):
            if token != 'macro_rules' or stream[i + 1:i + 3] != ['!', name]:
                continue
            if stream[i - 4:i] != ['#', '[', 'macro_export', ']']:
                return False
            body = i + 3
            if stream[body:body + 1] != ['{']:
                return False
            end = group_close(stream, body)
            k = body + 1
            while k < end - 1:
                matcher_end = group_close(stream, k)
                if stream[matcher_end:matcher_end + 2] != ['=', '>']:
                    return False
                transcriber = matcher_end + 2
                transcriber_end = group_close(stream, transcriber)
                j = transcriber + 1
                while j < transcriber_end - 1:
                    if stream[j] != 'impl':
                        return False
                    block = next(m for m in range(j, transcriber_end) if stream[m] == '{')
                    j = group_close(stream, block)
                k = transcriber_end + (stream[transcriber_end:transcriber_end + 1] == [';'])
                found = True
    return found


@functools.lru_cache(maxsize=None)
def engine_module_names(sdk_root, module):
    """Every name a `use safe_cex_engine::<module>::*` can bring in, or None when the
    module is not a plain, re-export-free, generator-free engine module."""
    try:
        lib = request_tokens((sdk_root / 'engine/src/lib.rs').read_text())
    except OSError:
        return None
    declared = any(lib[i:i + 4] == ['pub', 'mod', module, ';'] and lib[i - 1:i] != [']']
                   for i in range(len(lib)))
    candidates = [sdk_root / f'engine/src/{module}.rs', sdk_root / f'engine/src/{module}/mod.rs']
    files = [path for path in candidates if path.exists()]
    if not declared or len(files) != 1:
        return None
    stream = request_tokens(files[0].read_text())
    depth = brace_depths(stream)
    names = set()
    for name, kind, detail in binders(stream):
        if kind in {'macro_rules', 'macro', 'extern crate', 'unparsed use'}:
            return None
        names.add(name)
    for i, token in enumerate(stream):
        if token == 'use' and depth[i] == 0 and stream[i - 1:i] in (['pub'], [')']):
            return None
    if any(depth[i] == 0 for _, i in macro_invocations(stream)):
        return None
    return names


@functools.lru_cache(maxsize=None)
def engine_enum_variants(sdk_root, name):
    """Variants of the unique engine `pub enum <name>`, else None."""
    found = []
    for path in sorted((sdk_root / 'engine/src').glob('**/*.rs')):
        stream = request_tokens(path.read_text())
        for i, token in enumerate(stream):
            if token == 'enum' and stream[i + 1:i + 2] == [name] and stream[i - 1:i] == ['pub']:
                found.append(enum_variants(stream, i))
    return found[0] if len(found) == 1 else None


def brace_depths(stream):
    depth, result = 0, []
    for token in stream:
        if token == '}':
            depth -= 1
        result.append(depth)
        if token == '{':
            depth += 1
    return result


def crate_refusals(sdk_root, streams, watched, allowed_use=None, definitions=None):
    """Reasons the crate's sources cannot prove `watched` names unshadowed; [] when proven.

    `allowed_use(name, path)` admits a specific import of a watched name, and
    `definitions` maps a watched name to its one permitted (path, token index) item."""
    allowed_use = allowed_use or (lambda name, path: False)
    definitions = definitions or {}
    reasons = []
    crate_items, crate_variants = set(), collections.defaultdict(set)
    every_binder = []
    for path, stream in streams.items():
        found = binders(stream)
        every_binder += [(path, *entry) for entry in found]
        crate_items |= {name for name, kind, _ in found if kind in ITEMS}
        for i, token in enumerate(stream):
            if token == 'enum' and IDENT.fullmatch(stream[i + 1]):
                crate_variants[stream[i + 1]] |= set(enum_variants(stream, i))
    for path, name, kind, detail in every_binder:
        if kind == 'variant':
            continue  # A variant binds only under its enum's path, or through a glob of it.
        if kind == 'unparsed use':
            reasons.append(f'{path.name}: unparsed use')
        elif kind in {'macro_rules', 'macro'} and name in TRUSTED_MACROS:
            reasons.append(f'{path.name}: shadows trusted macro {name}')
        elif kind == 'use' and name in TRUSTED_MACROS | STD_DERIVES:
            reasons.append(f'{path.name}: imports {name}')
        elif kind == 'use' and name in SERDE_DERIVES and detail != ('serde', name):
            reasons.append(f'{path.name}: imports {name} from {"::".join(detail)}')
        elif name in watched and not (kind == 'use' and allowed_use(name, detail)) \
                and definitions.get(name) != (path, detail):
            reasons.append(f'{path.name}: {kind} binds {name}')
    for path, stream in streams.items():
        for i, token in enumerate(stream):
            if token != 'use':
                continue
            for use_path, leaf, aliased, glob in use_leaves(stream, i) or []:
                if not glob:
                    continue
                head = use_path[:1]
                if head and head[0] in crate_variants and len(use_path) == 1:
                    if watched & crate_variants[head[0]]:
                        reasons.append(f'{path.name}: glob {head[0]}::* exports a watched variant')
                    continue
                if head and use_path[-1] in crate_variants and watched & crate_variants[use_path[-1]]:
                    reasons.append(f'{path.name}: glob {"::".join(use_path)}::* exports a watched variant')
                    continue
                if head in (['crate'], ['super'], ['self']) or (head and head[0] in crate_items
                                                                   and head[0] not in watched):
                    continue
                if len(use_path) == 2 and use_path[0] == 'safe_cex_engine':
                    names = engine_module_names(sdk_root, use_path[1])
                    if names is None:
                        names = engine_enum_variants(sdk_root, use_path[1])
                elif len(use_path) == 1:
                    names = engine_enum_variants(sdk_root, use_path[0])
                    if names is not None and any(entry[1] == use_path[0] and entry[2] == 'use'
                                                 and entry[3][:1] != ('safe_cex_engine',)
                                                 for entry in every_binder):
                        names = None
                else:
                    names = None
                if names is None:
                    reasons.append(f'{path.name}: unproven glob {"::".join(use_path)}::*')
                elif watched & set(names):
                    reasons.append(f'{path.name}: glob {"::".join(use_path)}::* exports a watched name')
        for invocation, i in macro_invocations(stream):
            if len(invocation) == 1 and invocation[0] in TRUSTED_MACROS:
                continue
            if len(invocation) == 2 and invocation[0] in WORKSPACE_MACRO_CRATES \
                    and invocation[0] not in crate_items and impl_only_macro(sdk_root, *invocation):
                continue
            reasons.append(f'{path.name}: generator {"::".join(invocation)}!')
        for attribute, arguments in attributes(stream):
            if attribute[:1] == ['derive'] and len(attribute) == 1:
                derived = [t for t in arguments[1:-1] if t not in {','}]
                k = 0
                while k < len(derived):
                    if derived[k:k + 2] == ['serde', '::'] and derived[k + 2:k + 3] and derived[k + 2] in SERDE_DERIVES:
                        k += 3
                    elif derived[k] in STD_DERIVES | SERDE_DERIVES:
                        k += 1
                    else:
                        reasons.append(f'{path.name}: derive {derived[k]}')
                        break
            elif len(attribute) != 1 or attribute[0] not in TRUSTED_ATTRIBUTES:
                reasons.append(f'{path.name}: attribute {"::".join(attribute)}')
    return reasons
