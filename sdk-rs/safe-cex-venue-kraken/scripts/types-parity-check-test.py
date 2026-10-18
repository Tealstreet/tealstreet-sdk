"""Regression for complete list-valued mappings in the production wrapper."""
from copy import deepcopy
from pathlib import Path
import runpy
import unittest

wrapper = runpy.run_path(str(Path(__file__).with_name('types-parity-check.py')))
owned_rows = wrapper['owned_rows']
declared_files = wrapper['declared_files']


class SplitMappings(unittest.TestCase):
    def test_preserves_both_split_targets_and_scalar_and_finding_rows(self):
        split = {'ts': 'createAPI', 'rs': [
            'safe-cex-venue-kraken/src/api.rs::spot_create_api',
            'safe-cex-venue-kraken/src/api.rs::ApiHost::resend',
        ], 'note': 'Original split evidence must be retained.'}
        scalar = {'ts': 'KrakenBaseUrl', 'rs': 'safe-cex-venue-kraken/src/types.rs::KrakenBaseUrl', 'note': ''}
        finding = {'ts': 'open', 'rs': '', 'note': 'FINDING: unresolved source behavior'}
        rows = [split, scalar, finding, {'ts': 'shared', 'rs': ['safe-cex-crypto/src/lib.rs::sign'], 'note': ''},
                {'ts': 'pending', 'rs': '', 'note': ''}]
        before = deepcopy(rows)
        self.assertEqual(owned_rows(rows), [split, scalar, finding])
        self.assertEqual(rows, before)
        self.assertEqual(owned_rows(rows)[0]['rs'], [
            'safe-cex-venue-kraken/src/api.rs::spot_create_api',
            'safe-cex-venue-kraken/src/api.rs::ApiHost::resend',
        ])

    def test_mixed_split_preserves_committed_provenance_without_inventing_missing_files(self):
        rows = owned_rows([{'rs': [
            'safe-cex-venue-kraken/src/api.rs::spot_create_api',
            'safe-cex-crypto/src/lib.rs::sign',
        ], 'note': 'Original split.'}])
        declared = ['safe-cex-venue-kraken/src/api.rs',
                    'safe-cex-venue-kraken/src/unused.rs',
                    'safe-cex-crypto/src/lib.rs', 'safe-cex-types/src/lib.rs']
        self.assertEqual(declared_files(declared, rows), declared[:3])
        self.assertEqual(declared_files(declared[:2], rows), declared[:2])


if __name__ == '__main__':
    unittest.main()
