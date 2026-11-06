import type { HttpPacket, HttpResponse, PreparedHttp, Transport } from '@tealstreet/safe-cex-wasm/host';

import {
  isHeartbeatFrame,
  isWsDiagnosticsEnabled,
  redactWsDiagnosticPayload,
  WsDiagnosticSampler,
} from '../utils/wsDiagnostics';

/** The Requests Log events the TS (axios and BaseWebSocket) paths already emit. */
export type RequestsLogEvent = 'grafana-http-request' | 'grafana-http-response' | 'grafana-pws' | 'grafana-pws-sub';

const BODY_CHARS = 4096;
/** Enough of a body to redact as whole JSON before the log entry is capped. */
const OBSERVED_CHARS = BODY_CHARS * 4;

const capped = (value: string) =>
  value.length <= BODY_CHARS ? value : `${value.slice(0, BODY_CHARS)}...truncated (${value.length} chars)`;

/** Never headers: they carry keys, signatures, passphrases and proxy tokens. */
const describe = (method: string, url: string) => ({
  method: method.toUpperCase(),
  url: redactWsDiagnosticPayload(url),
});

const enabled = () => {
  try {
    return isWsDiagnosticsEnabled();
  } catch {
    return false;
  }
};

/**
 * Rust engine traffic into the Requests Log, through the same events and the
 * same toggle (`isWsDiagnosticsEnabled`) as the TS paths. Off, each request and
 * frame costs one flag check and builds nothing.
 *
 * Harmless by construction. The caller always gets the transport's own promise,
 * response object and stream, and each frame is delivered or sent before it is
 * logged. Logging reads only immutable strings or a copy of the bytes, and runs
 * later in `offPath`, whose one try/catch swallows every failure. A streamed
 * body is never read; its entry says so.
 */
export const withRequestsLog = (
  transport: Transport,
  label: string,
  emit: (event: RequestsLogEvent, message: string) => void,
): Transport => {
  const offPath = (work: () => void) => {
    try {
      transport.setTimeout(() => {
        try {
          work();
        } catch {
          // The entry is dropped; the traffic already went through.
        }
      }, 0);
    } catch {
      // No timer, no entry.
    }
  };
  const logged = (request: HttpPacket, send: () => Promise<HttpResponse>): Promise<HttpResponse> => {
    const response = send();
    if (!enabled()) return response;
    try {
      const started = Date.now();
      const { method, url } = request;
      const sent = typeof request.body === 'string' ? request.body : undefined;
      offPath(() => {
        const line = describe(method, url);
        const body = sent === undefined ? undefined : capped(redactWsDiagnosticPayload(sent));
        emit('grafana-http-request', `Request ${line.method} ${line.url} ${JSON.stringify({ ...line, body })}`);
      });
      // Registered before the caller can attach its own callbacks, so the copy
      // is taken before the caller consumes or transfers the bytes.
      response.then(
        (answer) => {
          try {
            const { status } = answer;
            const ms = Date.now() - started;
            const bytes = answer.body instanceof Uint8Array ? answer.body.slice(0, OBSERVED_CHARS) : undefined;
            offPath(() => {
              const line = describe(method, url);
              // Redact the whole body first: a capped one is no longer JSON.
              const body = bytes
                ? capped(redactWsDiagnosticPayload(new TextDecoder().decode(bytes)))
                : '(streamed body, not logged)';
              emit(
                'grafana-http-response',
                `Response ${line.method} ${line.url} ${status} ${JSON.stringify({ status, ms, body })}`,
              );
            });
          } catch {
            // The response entry is dropped.
          }
        },
        () => {
          // The caller sees the rejection on `response` itself.
        },
      );
    } catch {
      // The entries are dropped.
    }
    return response;
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
      // A text frame is an immutable string; a binary one is logged by its length.
      const frame = (event: RequestsLogEvent, data: string | number) => {
        if (!enabled()) return;
        offPath(() => {
          if ((typeof data === 'string' && isHeartbeatFrame(data)) || sampler.drop()) return;
          const message = typeof data === 'string' ? redactWsDiagnosticPayload(data) : `binary ${data} bytes`;
          sampler.emit((entry) => emit(event, entry), label, message);
        });
      };
      const socket = transport.connect(url, {
        ...handlers,
        onMessage: (message) => {
          handlers.onMessage(message);
          frame('grafana-pws', message);
        },
        ...(handlers.onBinary
          ? {
              onBinary: (data: Uint8Array) => {
                const length = data.byteLength;
                handlers.onBinary!(data);
                frame('grafana-pws', length);
              },
            }
          : {}),
      });
      return {
        send: (message: string) => {
          socket.send(message);
          frame('grafana-pws-sub', message);
        },
        close: () => socket.close(),
      };
    },
  };
};
