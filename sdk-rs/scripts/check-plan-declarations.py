#!/usr/bin/env python3
"""A `Venue::http_plans()` false declaration must cover code that opens no Preflight.

The engine compiles its per-event Plan sweeps out for a venue declaring false and
refuses (debug: panics on) any Preflight it opens. So the declaring impl's scope
may not reference a Preflight or Plan admission API: the impl's module directory
for a `mod.rs` impl, otherwise the whole crate source. A crate-root impl also
may not sit in a crate whose Cargo.toml enables a Plan engine feature.
"""
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]
DECLARES_FALSE = re.compile(r'fn\s+http_plans\s*\(\s*\)\s*->\s*bool\s*\{\s*false\s*\}')
PLAN_API = re.compile(
    r'\b(?:begin_preflight\w*|issue_preflight\w*|admit_serial_plan|admit_plan\w*'
    r'|begin_planned_execution)\s*\(')
PLAN_FEATURE = re.compile(
    r'"(?:http-plans|serial-plans|condition-waits|release-domains|write-preflight-wait'
    r'|http-mutation-barriers|repair-mutations)"')


def scope(path):
    """The source the declaring impl in `path` may reach."""
    src = next(parent for parent in path.parents if parent.name == 'src')
    return path.parent if path.name == 'mod.rs' else src


def violations(declaring, sources, cargo_toml):
    """`declaring`: impl files declaring false; `sources`: {path: text}."""
    found = []
    for path in sorted(declaring):
        reach = scope(path)
        for other, text in sorted(sources.items()):
            if reach in other.parents:
                for number, line in enumerate(text.splitlines(), 1):
                    if PLAN_API.search(line):
                        found.append(f'{path}: declares http_plans() false but '
                                     f'{other}:{number} opens a Preflight or Plan')
        if path.name != 'mod.rs' and PLAN_FEATURE.search(cargo_toml.get(reach.parent, '')):
            found.append(f'{path}: declares http_plans() false but '
                         f'{reach.parent / "Cargo.toml"} enables a Plan engine feature')
    return found


def main():
    venues = ROOT / 'venues'
    sources = {path.relative_to(ROOT): path.read_text() for path in venues.rglob('src/**/*.rs')}
    declaring = {path for path, text in sources.items() if DECLARES_FALSE.search(text)}
    cargo_toml = {path.parent.relative_to(ROOT): path.read_text()
                  for path in venues.glob('*/Cargo.toml')}
    found = violations(declaring, sources, cargo_toml)
    for line in found:
        print(f'BANNED: {line}')
    return 1 if found else 0


if __name__ == '__main__':
    sys.exit(main())
