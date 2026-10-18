#!/usr/bin/env node
// Print the exchange lists derived from cypress/config/exchanges.js as JSON.
// Used by the e2e workflow (mirrors/tealstreet-sdk/.github/workflows/e2e.yml) to build its matrix so CI's notion of
// "all" always matches the local runners'.
//
//   node cypress/scripts/list-exchanges.js            -> all configured exchanges
//   node cypress/scripts/list-exchanges.js --valid    -> every configured exchange

const { VALID_EXCHANGES, DEFAULT_EXCHANGES } = require('../config/exchangeLists.js');

const list = process.argv.includes('--valid') ? VALID_EXCHANGES : DEFAULT_EXCHANGES;
process.stdout.write(`${JSON.stringify(list)}\n`);
