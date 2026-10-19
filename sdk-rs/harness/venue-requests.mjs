import { requestKey } from './requests.mjs';
import { canonicalBody, canonicalUrl } from './tape.mjs';

/** Existing venues keep the strict typed oracle; MEXC accepts snowflake strings/numbers. */
export const venueRequestKey = (method, url, body, callerIds, venue) => {
  if (venue.rust !== 'mexc') return requestKey(method, url, body, callerIds, venue);
  // MEXC rejects caller clientOrderId and generates externalOid internally.
  // Its active trigger lists also use a rolling, fixed-width time window.
  return `${method} ${canonicalUrl(url)}${venue.matchRequestBody && method !== 'GET' ? ` ${canonicalBody(body)}` : ''}`;
};
