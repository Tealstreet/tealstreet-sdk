// Credential variants must differ from the primary e2e-<exchange> hosted account.
// Keep names stable across retries to reuse the same hosted account and journal.
const testAccountName = (exchange, variant) => `e2e-${exchange}-${variant}`;

module.exports = { testAccountName };
