// Reuse the maintained factory loader and its independent retry-budget corpus.
process.argv.push('--retry-budgets');
require('./create-api-signature-resend-boundaries.cjs');
