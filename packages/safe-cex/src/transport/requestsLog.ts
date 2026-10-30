import type { HttpPacket, HttpResponse, PreparedHttp, Transport } from '@tealstreet/safe-cex-wasm/host';

import {
  isHeartbeatFrame,
  isWsDiagnosticsEnabled,
  redactWsDiagnosticPayload,
  stringifyWsDiagnosticPayload,
  WsDiagnosticSampler,
} from '../utils/wsDiagnostics';

/** The Requests Log events the TS (axios and BaseWebSocket) paths already emit. */
export type RequestsLogEvent = 'grafana-http-request' | 'grafana-http-response' | 'grafana-pws' | 'grafana-pws-sub';

const BODY_CHARS = 4096;
const text = new TextDecoder();

const capped = (value: string) =>
  value.length <= BODY_CHARS ? value : `${value.slice(0, BODY_CHARS)}...truncated (${value.length} chars)`;

/** Never headers: they carry keys, signatures, passphrases and proxy tokens. */
const describe = (request: HttpPacket) => {
  const url = redactWsDiagnosticPayload(request.url);
  const body = typeof request.body === 'string' ? redactWsDiagnosticPayload(capped(request.body)) : undefined;
  return { method: request.method.toUpperCase(), url, body };
};

/** The response with its body observed, so the log entry is written once the body completes. */
const observed = (response: HttpResponse, done: (body: string) => void): HttpResponse => {
  if (response.body instanceof Uint8Array) {
    done(text.decode(response.body.subarray(0, BODY_CHARS * 4)));
    return response;
  }
  const chunks = response.body;
  return {
    ...response,
    body: (async function* () {
      let seen = '';
      try {
        for await (const chunk of chunks) {
          if (seen.length < BODY_CHARS) seen += text.decode(chunk, { stream: true });
          yield chunk;
        }
      } finally {
        done(seen);
      }
    })(),
  };
};

/**
 * Rust engine traffic into the Requests Log, through the same events and the
 * same toggle (`isWsDiagnosticsEnabled`) as the TS paths. Off, each request and
 * frame costs one flag check and builds nothing.
 */
export const withRequestsLog = (
  transport: Transport,
  label: string,
  emit: (event: RequestsLogEvent, message: string) => void,
): Transport => {
  const logged = (request: HttpPacket, send: () => Promise<HttpResponse>): Promise<HttpResponse> => {
    if (!isWsDiagnosticsEnabled()) return send();
    const { method, url, body } = describe(request);
    emit('grafana-http-request', `Request ${method} ${url} ${JSON.stringify({ method, url, body })}`);
    const started = Date.now();
    return send().then((response) =>
      observed(response, (raw) => {
        const summary = {
          status: response.status,
          ms: Date.now() - started,
          body: redactWsDiagnosticPayload(capped(raw)),
        };
        emit('grafana-http-response', `Response ${method} ${url} ${response.status} ${JSON.stringify(summary)}`);
      }),
    );
  };
  // Members are forwarded explicitly: a class instance's methods do not spread.
  const prepared = (inner: PreparedHttp): PreparedHttp => ({
    expiresAt: inner.expiresAt,
    bind: (packet) => {
      const bound = inner.bind(packet);
      return {
        ready: () => bound.ready(),
        dispatch: () => logged(packet, () => bound.dispatch()),
        ...(bound.cancel ? { cancel: () => bound.cancel!() } : {}),
      };
    },
    ...(inner.cancel ? { cancel: () => inner.cancel!() } : {}),
  });
  return {
    ...transport,
    request: (request) => logged(request, () => transport.request(request)),
    ...(transport.prepareHttp
      ? {
          prepareHttp: (request: Parameters<NonNullable<Transport['prepareHttp']>>[0]) => {
            const inner = transport.prepareHttp!(request);
            return inner instanceof Promise ? inner.then(prepared) : prepared(inner);
          },
        }
      : {}),
    connect: (url, handlers) => {
      // Market data is not the private stream the log is for.
      if ((handlers.channel ?? 'public') === 'public') return transport.connect(url, handlers);
      const sampler = new WsDiagnosticSampler();
      const frame = (event: RequestsLogEvent, data: string | Uint8Array) => {
        if (!isWsDiagnosticsEnabled() || (typeof data === 'string' && isHeartbeatFrame(data)) || sampler.drop()) {
          return;
        }
        const message = typeof data === 'string' ? redactWsDiagnosticPayload(data) : `binary ${data.byteLength} bytes`;
        sampler.emit((entry) => emit(event, entry), label, message);
      };
      const socket = transport.connect(url, {
        ...handlers,
        onMessage: (message) => {
          frame('grafana-pws', message);
          handlers.onMessage(message);
        },
        ...(handlers.onBinary
          ? {
              onBinary: (data: Uint8Array) => {
                frame('grafana-pws', data);
                handlers.onBinary!(data);
              },
            }
          : {}),
      });
      return {
        send: (message: string) => {
          frame('grafana-pws-sub', stringifyWsDiagnosticPayload(message));
          socket.send(message);
        },
        close: () => socket.close(),
      };
    },
  };
};
