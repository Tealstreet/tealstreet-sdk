import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location('causes', Path(__file__).with_name('diagnostic-causes.py'))
causes = importlib.util.module_from_spec(spec)
spec.loader.exec_module(causes)


class CauseTests(unittest.TestCase):
    def test_independent_nine_diagnostic_partition(self):
        lines = [
            'invention: sample/src/lib.rs::Host: public item has no manifest rs or exemption',
            'invention: sample/src/lib.rs::Host::send: public item has no manifest rs or exemption',
            'status: source.ts::Source.send (method): Rust item path must be Source::send: sample/src/lib.rs::send; a rename note must name this exact Rust target',
            'status: source.ts::create (function): Rust source file is not declared for source.ts: sample/src/factory.rs::create; declare the exact crate/file in [file].rust_files',
            'status: source.ts::legacy (function): rs does not resolve: sample::lib::legacy',
            'status: source.ts::legacy (function): Rust item path must be legacy: sample::lib::legacy; a rename note must name this exact Rust target',
            'status: source.ts::legacy (function): Rust source file is not declared for source.ts: sample::lib::legacy; declare the exact crate/file in [file].rust_files',
            'invention: sample/src/lib.rs::legacy: public item has no manifest rs or exemption',
            'invention: sample/src/lib.rs::unknown: public item has no manifest rs or exemption',
        ]
        result = causes.analyze(causes.records('\n'.join(lines)), {
            'sample/src/lib.rs::Host': 'trait',
            'sample/src/lib.rs::Host::send': 'trait-member',
        }, {})
        expected = {
            'unclaimed-trait-api': 2,
            'receiver-namespace-relocation': 1,
            'missing-source-file-declaration': 1,
            'legacy-crate-mapping': 4,
            'unassigned': 1,
        }
        self.assertEqual(result['diagnostics'], 9, 'CAUSE independently expected diagnostic floor')
        self.assertEqual({k: v['diagnostics'] for k, v in result['causes'].items()}, expected, 'CAUSE independently expected disjoint root groups')
        self.assertEqual(result['causes']['unclaimed-trait-api']['groups'], 1)

    def test_unknown_empty_or_unresolved_rename_is_not_a_confirmed_relocation(self):
        for text in ['', 'parity report refused: Rust-index validation failed', 'invention: changed format']:
            with self.assertRaises(ValueError):
                causes.records(text)
        text = '\n'.join([
            'status: source.ts::Source.send (method): rs does not resolve: sample/src/lib.rs::send',
            'status: source.ts::Source.send (method): Rust item path must be Source::send: sample/src/lib.rs::send; a rename note must name this exact Rust target',
        ])
        result = causes.analyze(causes.records(text), {}, {})
        self.assertEqual(result['causes']['unassigned']['diagnostics'], 2)
        self.assertNotIn('receiver-namespace-relocation', result['causes'])

    def test_trait_priority_and_legacy_layer_are_counted_once(self):
        text = '\n'.join([
            'status: source.ts::Host (type): rs does not resolve: sample::lib::Host',
            'invention: sample/src/lib.rs::Host: public item has no manifest rs or exemption',
            'status: source.ts::Host (type): nonempty rs requires layer',
            'status: source.ts::Other (type): nonempty rs requires layer',
        ])
        result = causes.analyze(causes.records(text), {'sample/src/lib.rs::Host': 'trait'}, {
            ('source.ts', 'Host', 'type'): {'rs': 'sample::lib::Host'},
            ('source.ts', 'Other', 'type'): {'rs': 'sample/src/lib.rs::Other'},
        })
        self.assertEqual({k: v['diagnostics'] for k, v in result['causes'].items()}, {
            'unclaimed-trait-api': 1, 'legacy-crate-mapping': 2, 'unassigned': 1,
        }, 'CAUSE overlapping syntax must partition once')
        self.assertEqual(result['legacy_unmapped_candidates'], 1)


if __name__ == '__main__':
    unittest.main()
