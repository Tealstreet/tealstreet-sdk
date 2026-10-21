import { isWriteRequest } from './requests.mjs';
import { tapeHeaders } from './tape-fields.mjs';

// Polling transports have HTTP activity and loaded state, without a socket.
export const replayTransportReady = (venue, openSockets, loaded = true) =>
  loaded && (venue.polling === true || openSockets > 0);
export const deterministicReplay = (venue, isPrivate) => venue.polling === true || venue.virtualPrivateClock === true || !isPrivate;
// Public/polling startup can finish at a different wall time when protocol
// auditing is enabled. Keep its pending poll deadlines anchored to tape epoch,
// just like an explicitly virtual private replay, instead of carrying that
// startup wall-time jitter into the measured transport workload.
export const frozenReplayStartup = (venue, isPrivate) => !isPrivate || venue.polling === true || venue.virtualPrivateClock === true;
// A venue whose private engine paces REST by absolute deadlines keeps a
// monotonic clock: its tape epoch follows the virtual time startup consumed.
export const continuousReplayStartup = (venue, isPrivate) => isPrivate && venue.virtualPrivateClock === true && venue.monotonicReplayClock === true;
export const recordingReady = (venue, { frames, httpResponses, loaded, books, requestedBooks }) =>
  (venue.polling === true ? httpResponses > 0 && loaded : frames > 0) && books >= requestedBooks;

// Replay starts its clock after startup. Keep pre-ready retries in their ordered,
// unstamped sequence; stamping a later successful retry would deadlock startup.
// Later snapshots retain their actual response time relative to tape epoch; the
// final-anchor fence waits for a natural poll if startup shifted its phase.
export const createRecordingTimeline = (venue) => (record, ready = false) => {
  if (!ready || venue.polling !== true || record.type !== 'http'
      || (venue.isWrite?.(record.method, record.url) ?? isWriteRequest(record.method, record.url))) return record;
  return { ...record, availableAfter: record.t };
};

const credentialKey = /^(?:key|secret|password|username|passphrase|sessionToken|access_token|refresh_token)$/i;
const collectSecrets = (value, secrets) => {
  if (!value || typeof value !== 'object') return;
  for (const [key, entry] of Object.entries(value)) {
    if (credentialKey.test(key) && typeof entry === 'string' && entry) secrets.add(entry);
    else collectSecrets(entry, secrets);
  }
};

// Session recordings require a venue-owned auth substitution, never generic redaction
// that would make a login replay succeed without checking its real wire contract.
export const createRecordingPolicy = (venue, options, credentialed = false) => {
  const session = venue.recordSession === true || credentialed;
  if (session && (!venue.sanitizeRecordingRecord || !venue.recordReplayOptions))
    throw new Error('Credentialed recording requires replay-safe auth sanitizers');
  const secrets = new Set();
  collectSecrets(options, secrets);
  const assertSafe = (value) => {
    const inspect = (entry, depth = 0) => {
      if (depth > 64) throw new Error('Recording record nesting exceeds the safety limit');
      if (typeof entry === 'string') {
        let normalized = entry;
        for (let pass = 0; pass < 64; pass++) {
          for (const secret of secrets) {
            if ([secret, encodeURIComponent(secret)].some((part) => normalized.includes(part)))
              throw new Error('Recording sanitizer retained a live credential or session token');
          }
          const decoded = normalized.replace(/\\(?:u([a-fA-F0-9]{4})|(["\\/bfnrt]))/g, (_whole, unicode, escaped) =>
            unicode ? String.fromCharCode(Number.parseInt(unicode, 16)) : ({ b: '\b', f: '\f', n: '\n', r: '\r', t: '\t' }[escaped] ?? escaped));
          if (decoded === normalized) break;
          if (pass === 63) throw new Error('Recording string escaping exceeds the safety limit');
          normalized = decoded;
        }
        for (const secret of secrets) {
          if ([secret, encodeURIComponent(secret)].some((part) => entry.includes(part)))
            throw new Error('Recording sanitizer retained a live credential or session token');
        }
        let parsed;
        try { parsed = JSON.parse(entry); } catch { return; }
        if (parsed !== entry) inspect(parsed, depth + 1);
      } else if (entry && typeof entry === 'object') {
        for (const [key, nested] of Object.entries(entry)) { inspect(key, depth + 1); inspect(nested, depth + 1); }
      }
    };
    inspect(value);
    return value;
  };
  const replayOptions = session ? assertSafe(venue.recordReplayOptions(options)) : undefined;
  if (session && (!replayOptions || typeof replayOptions !== 'object' || Array.isArray(replayOptions)))
    throw new Error('Credentialed recording requires explicit replay options');
  return {
    replayOptions,
    sanitize(record) {
      if (!session) return Object.hasOwn(record, 'headers') ? { ...record,
        headers: tapeHeaders(record.headers).map(([name, value]) => `${name}: ${value}`).join('\n') } : record;
      for (const field of ['body', 'requestBody']) {
        try { collectSecrets(JSON.parse(record[field]), secrets); } catch { /* Not JSON. */ }
      }
      const safe = venue.sanitizeRecordingRecord({ ...record, headers: '' }, options);
      if (!safe || typeof safe !== 'object' || safe.type !== record.type)
        throw new Error('Recording sanitizer must preserve each protocol record');
      return assertSafe(safe);
    },
    log(level, message) {
      if (session) {
        try { assertSafe(String(message)); }
        catch { console.error(`[${level}] Session recording diagnostic redacted`); return; }
      }
      console.error(`[${level}] ${String(message)}`);

    },
  };
};
