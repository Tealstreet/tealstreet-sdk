// The Node gates' transport. A browser dials with the runtime's own WebSocket;
// Node's `nativeTransport` dials with the `ws` library, which buffers a slow
// consumer in userland until a high-rate venue (OKX EEA) drops it. Gates use
// the global WebSocket whenever a socket needs no upgrade headers.
import { nativeTransport, socketMessages } from '../hosts/wasm/js/host.mjs';

export const gateTransport = (transport = nativeTransport(), Global = globalThis.WebSocket) => {
  if (typeof Global !== 'function') return transport;
  const headed = transport.connect.bind(transport);
  return {
    ...transport,
    connect(url, handlers) {
      if (Object.keys(handlers.headers ?? {}).length) return headed(url, handlers);
      const { binary = false, onOpen, onMessage, onBinary, onClose } = handlers;
      const ws = new Global(url);
      let closed = false;
      const reportClose = (code) => { if (!closed) { closed = true; onClose(code); } };
      ws.addEventListener('error', () => reportClose(1006));
      ws.addEventListener('open', () => { if (!closed) onOpen(); });
      const receive = socketMessages((data) => {
        if (!closed) { if (typeof data === 'string') onMessage(data); else onBinary?.(data); }
      }, () => ws.close(), binary);
      ws.addEventListener('message', (event) => receive(event.data));
      ws.addEventListener('close', (event) => reportClose(event.code));
      return { send: (text) => ws.send(text), close: () => ws.close() };
    },
  };
};
