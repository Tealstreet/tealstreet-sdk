// Every engine notice (log line or command result) must be free of the account's
// credentials and of signed request material; private replays fail otherwise.

const CREDENTIAL_FIELD = /key|secret|passphrase|seed|private/i;
// Shorter values (a key index, a placeholder) would match ordinary text.
const MIN_CREDENTIAL_LENGTH = 8;
const SIGNED_QUERY = /[?&](signature|sign|sig)=/i;

/** Credential values in `options`, including nested account option objects. */
export function credentialValues(options) {
  const values = new Set();
  const visit = (value, field) => {
    if (typeof value === 'string') {
      if (CREDENTIAL_FIELD.test(field) && value.length >= MIN_CREDENTIAL_LENGTH) values.add(value);
    } else if (value && typeof value === 'object') {
      for (const [key, nested] of Object.entries(value)) visit(nested, key);
    }
  };
  visit(options, '');
  return [...values];
}

/** One description per notice that quotes a credential or a signed query. */
export function findNoticeLeaks(notices, credentials) {
  const leaks = [];
  for (const notice of notices) {
    const text = typeof notice === 'string' ? notice : JSON.stringify(notice);
    if (text === undefined) continue;
    const credential = credentials.find((value) => text.includes(value));
    if (credential) leaks.push(`credential in notice: ${text.replaceAll(credential, '<credential>').slice(0, 200)}`);
    else if (SIGNED_QUERY.test(text)) leaks.push(`signed query in notice: ${text.slice(0, 200)}`);
  }
  return leaks;
}
