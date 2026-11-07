import { describe, expect, it, vi } from 'vitest';

import { HOWWebSocket } from './HOWWebSocket';

class DiagnosticSocket extends HOWWebSocket {
  reportError(event: Event) {
    this.handleWebSocketError(event);
  }
}

describe('HOW send failure diagnostics', () => {
  it('does not put signed URLs or payload credentials in the not-ready error', () => {
    const socket = Object.create(HOWWebSocket.prototype);
    socket.ws = { readyState: 3 };
    const data = {
      type: 'rpc.http',
      arg: {
        method: 'GET',
        url: 'https://api.example.com/private?sign=fixture-signature',
        headers: { key: 'fixture-key' },
      },
    };
    let failure: any;
    try {
      socket.validateAndSend(JSON.stringify(data), data);
    } catch (error) {
      failure = error;
    }
    expect(failure?.name).toBe('WebSocketNotReadyError');
    expect(failure.message).toContain('CLOSED');
    expect(failure.messagePreview).toContain('https://api.example.com/private');
    expect([failure.message, JSON.stringify(failure)].join(' ')).not.toMatch(/fixture-|\?sign/);
  });
});

it('reports a routine reconnect as diagnostic info, retaining terminal errors for exhausted retries', async () => {
  const info = vi.spyOn(console, 'info').mockImplementation(() => {});
  const error = vi.spyOn(console, 'error').mockImplementation(() => {});
  const socket = new HOWWebSocket('wss://tunnel.test/tunnel');
  try {
    await socket.onClose({ code: 1006 });
    expect(info).toHaveBeenCalledWith('[HOWWebSocket] WebSocket connection disconnected, reconnecting...');
    expect(error).not.toHaveBeenCalled();
    socket.ws = { loopState: { phase: 'stopped' } } as never;
    await socket.onClose({ code: 4000 });
    expect(error).toHaveBeenCalledWith('[HOWWebSocket] Reconnect retries exhausted');
  } finally {
    vi.restoreAllMocks();
  }
});

it('surfaces repeated connection failures once per retry streak', async () => {
  vi.useFakeTimers();
  const error = vi.spyOn(console, 'error').mockImplementation(() => {});
  vi.spyOn(console, 'info').mockImplementation(() => {});
  const socket = new HOWWebSocket('wss://tunnel.test/tunnel');
  socket.ws = { loopState: { phase: 'retry-scheduled' }, retryCount: 3 } as never;
  try {
    await socket.onClose({ code: 1006 });
    await socket.onClose({ code: 1006 });
    expect(error).toHaveBeenCalledTimes(1);
    expect(error).toHaveBeenCalledWith('[HOWWebSocket] Repeated connection failures; retrying');
  } finally {
    vi.clearAllTimers();
    vi.useRealTimers();
    vi.restoreAllMocks();
  }
});

it('also reports repeated URL-provider failures that emit error without close', () => {
  const error = vi.spyOn(console, 'error').mockImplementation(() => {});
  vi.spyOn(console, 'info').mockImplementation(() => {});
  const socket = new DiagnosticSocket('wss://tunnel.test/tunnel');
  socket.ws = { loopState: { phase: 'connecting' }, retryCount: 3 } as never;
  try {
    socket.reportError(new Event('error'));
    socket.reportError(new Event('error'));
    expect(error).toHaveBeenCalledOnce();
    expect(error).toHaveBeenCalledWith('[HOWWebSocket] Repeated connection failures; retrying');
  } finally {
    vi.restoreAllMocks();
  }
});

it('does not claim a stopped connection is still retrying after its terminal close', () => {
  const error = vi.spyOn(console, 'error').mockImplementation(() => {});
  vi.spyOn(console, 'info').mockImplementation(() => {});
  const socket = new DiagnosticSocket('wss://tunnel.test/tunnel');
  socket.ws = { loopState: { phase: 'stopped' }, retryCount: 3 } as never;
  try {
    socket.reportError(new Event('error'));
    expect(error).not.toHaveBeenCalled();
  } finally {
    vi.restoreAllMocks();
  }
});
