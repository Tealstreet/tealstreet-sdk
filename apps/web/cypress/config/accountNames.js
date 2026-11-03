// Hosted accounts are keyed by user and name, so parallel venues need distinct names.
// Keep names stable across retries to reuse the same hosted account and journal.
const testAccountName = (exchange, variant = 'primary') =>
  variant === 'primary' ? `test-${exchange}` : `test-${exchange}-${variant}`;

module.exports = { testAccountName };
