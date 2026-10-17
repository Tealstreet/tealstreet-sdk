#!/usr/bin/env python3
"""A name suffix may share exact guard bytes; only the selected owner may change."""
import importlib.util
from pathlib import Path
import unittest

path = Path(__file__).with_name('accepted-map-record-controls.py')
spec = importlib.util.spec_from_file_location('map_controls', path)
controls = importlib.util.module_from_spec(spec)
spec.loader.exec_module(controls)


class GuardIsolation(unittest.TestCase):
    def test_matching_suffix_with_identical_formatting_changes_one_owner(self):
        guard = b'::deserialize(safe_cex_types::strict_decode::object(decoder))'
        first = b'CurrencyObjectWire' + guard
        sibling = b'PerpetualCurrencyObjectWire' + guard
        source = first + b';\n' + sibling
        actual = controls.guard_removed(source, 'CurrencyObject')
        self.assertEqual(actual, b'CurrencyObjectWire::deserialize(decoder);\n' + sibling)
        actual = controls.guard_removed(source, 'PerpetualCurrencyObject')
        self.assertEqual(actual, first + b';\nPerpetualCurrencyObjectWire::deserialize(decoder)')

    def test_wrapped_call_changes_only_its_span(self):
        source = (b'prefix\nHistoryFundingObjectWire::deserialize(safe_cex_types::strict_decode::object(\n'
                  b'    decoder,\n))\nsuffix')
        self.assertEqual(controls.guard_removed(source, 'HistoryFundingObject'),
                         b'prefix\nHistoryFundingObjectWire::deserialize(decoder)\nsuffix')

    def test_missing_or_ambiguous_owner_refuses_before_mutation(self):
        call = b'CurrencyObjectWire::deserialize(safe_cex_types::strict_decode::object(decoder))'
        for source in (b'', call + b';\n' + call):
            with self.assertRaises(AssertionError):
                controls.guard_removed(source, 'CurrencyObject')


if __name__ == '__main__':
    unittest.main()
