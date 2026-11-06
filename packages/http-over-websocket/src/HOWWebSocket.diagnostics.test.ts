import { describe, expect, it } from 'vitest';

import { HOWWebSocket } from './HOWWebSocket';

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
