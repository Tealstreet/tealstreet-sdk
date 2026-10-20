# Per-family WASM sizes

Fresh Rust build `7bc9e30d32`; JS host repair `8150295a7c` leaves all WASM artifacts unchanged. Node and web bytes match for every family. The original landing control builds a 12,174,466-byte aggregate module.

Incoming shared-public-feed and consumer functionality exceeds 26 previous family caps. Rebaseline only those breaches to measured bytes plus 3%, rounded up to KiB. Keep the fitting Paradex cap unchanged. These are per-family CI limits; CPU, memory, output, GC and timeout gates are unchanged.

| Family | WASM bytes | Previous cap | New cap |
| --- | ---: | ---: | ---: |
| apexomni | 1,083,189 | 1,071,104 | 1,116,160 |
| asterdex | 1,126,167 | 1,096,704 | 1,160,192 |
| backpack | 870,591 | 856,064 | 897,024 |
| binance | 1,008,470 | 960,512 | 1,039,360 |
| bingx | 944,500 | 931,840 | 973,824 |
| bitget | 1,040,624 | 1,029,120 | 1,072,128 |
| bitrue | 889,075 | 805,888 | 916,480 |
| blofin | 891,271 | 876,544 | 918,528 |
| breakoutprop | 758,842 | 724,992 | 782,336 |
| bybit | 1,192,342 | 1,174,528 | 1,228,800 |
| coinbase | 1,081,257 | 1,058,816 | 1,114,112 |
| cryptocom | 759,419 | 740,352 | 782,336 |
| grvt | 1,078,647 | 1,060,864 | 1,111,040 |
| hyperliquid | 1,135,345 | 1,090,560 | 1,169,408 |
| kraken | 1,151,202 | 1,118,208 | 1,185,792 |
| kucoin | 855,652 | 840,704 | 881,664 |
| kucoinspot | 760,047 | 708,608 | 783,360 |
| lighter | 1,080,607 | 1,065,984 | 1,113,088 |
| mexc | 878,314 | 864,256 | 905,216 |
| okx | 900,892 | 880,640 | 928,768 |
| orderly | 889,870 | 878,592 | 917,504 |
| paradex | 1,237,634 | 1,311,744 | 1,311,744 |
| phemex | 852,054 | 829,440 | 878,592 |
| polymarket | 932,347 | 902,144 | 960,512 |
| txflow | 1,029,760 | 1,003,520 | 1,060,864 |
| weex | 891,037 | 884,736 | 918,528 |
| woo | 944,658 | 931,840 | 973,824 |

Physical release proof SHA `725379d643a2852301e35a6deb1c2ae1598953e3dbf838b5f4f9745b322bf014`; independent audit SHA `c0aef86eb03fe06316ce1290e150e99f75f4ab39d6efd344b177e3c5917d5a2e`. The original failing official size check is retained; the official check passes all 54 Node/web entries after rebaselining. CI must also pass these limits. This table does not establish replay performance acceptance.
