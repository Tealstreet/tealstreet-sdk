import argparse
import os
from pathlib import Path
import re
import subprocess


ROOT = Path(__file__).resolve().parents[3]
CRATE = Path(__file__).resolve().parents[1]
PRODUCTION = CRATE / 'src/order_reads.rs'
TEST = CRATE / 'tests/order_reads.rs'
ROLES = ('normal', 'algo', 'combined', 'normal-fallback', 'algo-fallback')


def without_source_guards(source):
    while match := re.search(r'KUCOIN-CARDINALITY[^"\n]*source[^"\n]*', source):
        start = source.rfind('assert_eq!(', 0, match.start())
        if start < 0:
            raise RuntimeError('source guard has no assertion')
        end = source.index(');', match.end()) + 2
        source = source[:start] + source[end:]
    return source


def mutate(source, role, variant):
    limit = 0 if variant.endswith('empty') else 1
    project = ('orders.iter().chain(orders.iter()).cloned().collect()'
               if variant == 'extra' else f'orders.into_iter().take({limit}).collect()')
    if role in ('normal', 'algo'):
        flag = 'false' if role == 'normal' else 'true'
        old = f'read_book(base, xhr, clock, {flag}).await'
        new = f'{old}.map(|rows| rows.map(|orders| {project}))'
    elif role == 'combined':
        old = 'normal.into_iter().chain(algo).collect()'
        new = ('{ let orders: Vec<Order> = ' + old + '; ' + project + ' }')
    else:
        predicate = '.filter(|o| !is_stop(o))' if role == 'normal-fallback' else '.filter(is_stop)'
        old = predicate + '\n            .cloned()\n            .collect()'
        operation = '.flat_map(|order| [order.clone(), order])' if variant == 'extra' else f'.take({limit})'
        new = predicate + '\n            .cloned()\n            ' + operation + '.collect()'
    if source.count(old) != 1:
        raise RuntimeError(f'{role}: mutation target count {source.count(old)}')
    return source.replace(old, new)


def run(role, variant, expected):
    command = ['cargo', 'test', '--locked', '--manifest-path', str(ROOT / 'sdk-rs/Cargo.toml'),
               '-p', 'safe-cex-venue-kucoin', '--jobs', '1', '--test', 'order_reads',
               'order_book_cardinality_supplement_retains_each_category', '--', '--exact']
    result = subprocess.run(command, cwd=ROOT, env=os.environ.copy(), text=True,
                            stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    index = {'normal': 0, 'algo': 1, 'combined': 2, 'normal-fallback': 3, 'algo-fallback': 3}[role]
    assertion = f'KUCOIN-CARDINALITY book native category {index}'
    if result.returncode != expected or (expected == 101 and assertion not in result.stdout):
        raise RuntimeError(f'{role} {variant}: unexpected {result.returncode}\n{result.stdout}')
    print(f'{role} {variant}: {result.returncode}' + (f' at {assertion}' if expected else ''), flush=True)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('roles', nargs='+', choices=ROLES)
    args = parser.parse_args()
    production, test = PRODUCTION.read_bytes(), TEST.read_bytes()
    for role in args.roles:
        run(role, 'baseline', 0)
        try:
            for variant in ('empty', 'partial', 'extra', 'bypass-empty', 'bypass-partial'):
                PRODUCTION.write_text(mutate(production.decode(), role, variant))
                TEST.write_text(without_source_guards(test.decode()) if variant.startswith('bypass') else test.decode())
                run(role, variant, 101)
        finally:
            PRODUCTION.write_bytes(production)
            TEST.write_bytes(test)
        run(role, 'restored', 0)


if __name__ == '__main__':
    main()
