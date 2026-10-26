"""Conservative lexical name resolution for the inventory checkers.

`resolve` answers whether a bare type name at one token resolves to one exact
engine path. Brace blocks are scopes. The innermost enclosing scope that names
the type decides: an explicit unaliased import of the engine path is
'provider'; any other import, alias, item binder or generic parameter is
'other'; a scope reached only by globs is 'unknown'. An explicit import beats a
glob in the same scope, as in Rust. Anything unparsed stays conservative.
"""

ITEMS = {'struct', 'enum', 'type', 'trait', 'union', 'mod', 'fn', 'const', 'static'}
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
