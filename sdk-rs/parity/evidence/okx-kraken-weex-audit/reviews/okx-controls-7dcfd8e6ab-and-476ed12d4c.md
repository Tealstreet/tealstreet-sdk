Initial input: 7dcfd8e6ab143aebc1ca07979dbc4d47176a14dc
Reviewer: native sub-agent review_polymarket
Initial verdict: DO NOT MERGE until stale OKX citations are corrected.

The initial README's ticker, mark, index and funding TS lines, mark/index native declaration lines, lookup table lines and history table lines were stale. Probes selected the correct actual AST members. The reviewer inspected all twenty mutant logs: named AssertionError, not setup failure; all twenty 0/1/0 controls and all sixty-one published log hashes matched. No other blocker was found within the fixed thirty-six-row scope.

Resolution input: 476ed12d4c (documentation/static-receipt-only descendant)
Final verdict: MERGE within the fixed thirty-six-row note audit.

All previously blocking citations now match frozen d74a1c01f7. The source/control scripts remain unchanged from 3f2d53594f. Five static receipts retain actual input 7dcfd8e6ab and published hashes match. Native parity and WEEX refresh timing remain explicitly unproved. Reviewer ran no probes, Cargo, controls or other gates. All readers stopped.
