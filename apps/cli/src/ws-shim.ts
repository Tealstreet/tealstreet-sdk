/**
 * WebSocket shim for Bun CLI environment.
 *
 * Bun's native WebSocket doesn't support ALPNProtocols, which causes
 * "Expected 101 status code" errors when connecting to CloudFront-backed
 * WebSocket proxies that negotiate HTTP/2. Bun also replaces the `ws` npm
 * package with its own native implementation, so we can't use `ws` directly.
 *
 * This shim overrides globalThis.WebSocket so that connections to
 * tealstreet.io domains use `tcp-websocket` (undici's WebSocket, which
 * bypasses Bun's native shim), while all other connections use Bun's
 * native WebSocket for performance.
 */

const isBun = typeof process !== 'undefined' && !!(process as any).isBun;

if (isBun) {
  const NativeWebSocket = globalThis.WebSocket;

  // Lazy-load tcp-websocket only when needed
  let TcpWebSocket: any = null;

  const getTcpWebSocket = () => {
    if (!TcpWebSocket) {
      // Dynamic require to avoid loading undici for non-proxy connections
      // eslint-disable-next-line @typescript-eslint/no-require-imports
      const mod = require('tcp-websocket');
      TcpWebSocket = mod.default || mod;
    }
    return TcpWebSocket;
  };

  const isTealstreetProxy = (url: string): boolean => {
    try {
      const hostname = new URL(url).hostname;
      return hostname.endsWith('.tealstreet.io');
    } catch {
      return false;
    }
  };

  globalThis.WebSocket = function ShimmedWebSocket(url: string | URL, protocols?: string | string[]) {
    const urlStr = url.toString();

    if (isTealstreetProxy(urlStr)) {
      const Ws = getTcpWebSocket();
      return new Ws(urlStr, protocols || []);
    }

    // Native Bun WebSocket for everything else
    if (protocols) {
      return new NativeWebSocket(urlStr, protocols);
    }
    return new NativeWebSocket(urlStr);
  } as unknown as typeof WebSocket;

  // Copy static properties — `CONNECTING/OPEN/CLOSING/CLOSED` are `readonly`
  // on the WebSocket constructor type; we're intentionally shimming a global,
  // so a local `any` view onto the constructor is the right escape hatch.
  const shim = globalThis.WebSocket as any;
  shim.CONNECTING = NativeWebSocket.CONNECTING;
  shim.OPEN = NativeWebSocket.OPEN;
  shim.CLOSING = NativeWebSocket.CLOSING;
  shim.CLOSED = NativeWebSocket.CLOSED;
  shim.prototype = NativeWebSocket.prototype;
}
