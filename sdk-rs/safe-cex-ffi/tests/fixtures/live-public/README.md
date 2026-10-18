# Recorded public market responses

Captured 2026-10-18, without credentials. These bytes are replayed to the maintained
TypeScript exchange and the compiled native addon by `native-public-markets.test.mjs`.
The original live-diff reports retained response hashes but no bodies; these fresh
recordings reproduce their missing-market-field findings.

| Fixture | Public GET | SHA-256 |
| --- | --- | --- |
| `backpack-markets.json` | https://api.backpack.exchange/api/v1/markets | `80def488230e5cea02e0a712c8b20adb900984f307d5a5f27ffdd05962667251` |
| `cryptocom-markets.json` | https://api.crypto.com/exchange/v1/public/get-instruments | `93e8e3e27d1d895938f2a3a1ece58b5081abf8929e4605169e3c205d154846f0` |
