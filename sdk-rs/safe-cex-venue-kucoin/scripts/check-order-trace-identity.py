from importlib.util import module_from_spec, spec_from_file_location
from pathlib import Path


path = Path(__file__).with_name('check-spot-order-cardinality.py')
spec = spec_from_file_location('spot_order_controls', path)
controls = module_from_spec(spec)
spec.loader.exec_module(controls)


def main():
    targets = [
        ('concurrency_return_and_sibling_effects', 0, 'success'),
        ('concurrency_return_and_sibling_effects', 1, 'bad-code'),
        ('later_pages_rejections_and_info_alias', 4, 'transport'),
    ]
    for target, case, label in targets:
        for phase in ('atReturn', 'after'):
            for mode in ('empty', 'partial', 'extra', 'reverse', 'replace'):
                controls.run(target, f'cases.{case}.{phase}.events', mode,
                             f'KUCOIN-TRACE {label} {phase} event identity')
        controls.run(target, '', 'restored', '')


if __name__ == '__main__':
    main()
