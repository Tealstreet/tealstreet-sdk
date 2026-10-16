from importlib.util import module_from_spec, spec_from_file_location
from pathlib import Path


path = Path(__file__).with_name('check-spot-order-cardinality.py')
spec = spec_from_file_location('spot_order_controls', path)
controls = module_from_spec(spec)
spec.loader.exec_module(controls)


def main():
    target = 'concurrency_return_and_sibling_effects'
    for path, label in [
        ('mutatedStore', 'mutated Store'),
        ('afterReplacement.result', 'retained result'),
        ('afterReplacement.store', 'replacement Store'),
    ]:
        for mode in ('empty', 'extra', 'first-null'):
            controls.run(target, f'cases.1.{path}', mode,
                         f'KUCOIN-ALIAS {label} complete record identity')
    controls.run(target, '', 'restored', '')


if __name__ == '__main__':
    main()
