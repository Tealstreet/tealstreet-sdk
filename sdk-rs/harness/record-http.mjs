// HTTP capture for the live recorder. A venue on managed HTTP sends through
// `prepareHttp` (bind, then dispatch), never `request`, so both are recorded.
import { tapeHeaders } from './tape-fields.mjs';

/**
 * @param live the live transport being recorded
 * @param venue the harness venue entry (`recordPublicCorsDirect`)
 * @param write appends one tape record
 * @param onResponse called with the status of every recorded response
 */
export function recordingHttp(live, venue, write, onResponse) {
  // Concurrent equal requests can finish out of order (e.g. socket tokens).
  let requestOrdinal = 0;
  // A native recorder may explicitly declare direct egress for public CORS
  // reads. Browser/account hosts still require their configured route.
  const egress = (request) => request.route === 'publicCors' && venue.recordPublicCorsDirect
    ? { ...request, route: 'direct' }
    : request;
  const capture = (request, response, ordinal) => {
    const recorded = (body) => {
      write({
        type: 'http',
        requestOrdinal: ordinal,
        method: request.method,
        url: request.url,
        ...(request.body === undefined ? {} : { requestBody: request.body }),
        status: response.status,
        headers: tapeHeaders(response.headers).map(([name, value]) => `${name}: ${value}`).join('\n'),
        body,
      });
      onResponse(response.status);
    };
    if (response.body instanceof Uint8Array) {
      recorded(new TextDecoder().decode(response.body));
      return response;
    }
    return {
      ...response,
      body: (async function* () {
        const decoder = new TextDecoder();
        let body = '';
        for await (const chunk of response.body) {
          body += decoder.decode(chunk, { stream: true });
          yield chunk;
        }
        recorded(body + decoder.decode());
      })(),
    };
  };
  return {
    async request(request) {
      const ordinal = requestOrdinal++;
      return capture(request, await live.request(egress(request)), ordinal);
    },
    prepareHttp: typeof live.prepareHttp !== 'function' ? undefined : async (request) => {
      const prepared = await live.prepareHttp(egress(request));
      return {
        expiresAt: prepared.expiresAt,
        cancel: () => prepared.cancel?.(),
        bind(packet) {
          const bound = prepared.bind(packet);
          return {
            ready: () => bound.ready(),
            cancel: () => bound.cancel?.(),
            async dispatch() {
              const ordinal = requestOrdinal++;
              return capture(packet, await bound.dispatch(), ordinal);
            },
          };
        },
      };
    },
  };
}
