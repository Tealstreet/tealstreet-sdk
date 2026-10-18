// Derived exchange lists from exchanges.js — the single definition of what
// `--exchanges all` / the CI `all` matrix means. Both the local runners and
// .github/workflows/e2e.yaml read this so the two cannot drift apart.

const exchangeConfigs = require('./exchanges.js');

const VALID_EXCHANGES = Object.keys(exchangeConfigs).filter((k) => !k.startsWith('_'));
const DEFAULT_EXCHANGES = VALID_EXCHANGES;

module.exports = { VALID_EXCHANGES, DEFAULT_EXCHANGES };
