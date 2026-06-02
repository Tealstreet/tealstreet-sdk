import type { Server } from 'bun';
import type { StandalonePlatformAdapter } from '../platform-adapter';
import { buildWebSocketHandler, type WsSessionData } from './ws';
import type { ProcessCommandFn } from './handlers/cmd';
import { handleHookFire } from './handlers/hook';

export function buildServeOptions({
  port,
  adapter,
  listenerId,
  processCommand,
  notifyOwnAccountWrite,
  notifyOwnHookWrite,
  notifyOwnCrossTradeWrite,
  onClientConnect,
}: {
  port: number;
  adapter: StandalonePlatformAdapter;
  listenerId: string;
  processCommand: ProcessCommandFn;
  notifyOwnAccountWrite: () => void;
  notifyOwnHookWrite: () => void;
  notifyOwnCrossTradeWrite: () => void;
  onClientConnect?: () => void;
}) {
  return {
    hostname: '127.0.0.1',
    port,
    async fetch(req: Request, server: Server<WsSessionData>): Promise<Response | undefined> {
      const url = new URL(req.url);
      if (req.method === 'GET' && url.pathname === '/health') {
        return Response.json({ ok: true, pid: process.pid, port });
      }
      if (req.method === 'POST' && url.pathname.startsWith('/hook/')) {
        const hookId = url.pathname.slice('/hook/'.length);
        const token = req.headers.get('x-hook-token') ?? '';
        const result = await handleHookFire(server, hookId, token, processCommand);
        return Response.json(result.body, { status: result.status });
      }
      if (url.pathname === '/ws') {
        const origin = req.headers.get('origin') ?? '';
        const data: WsSessionData = {
          origin,
          trusted: false,
          helloSeen: false,
          webCrossTradeActive: false,
        };
        const upgraded = server.upgrade(req, { data });
        if (upgraded) return undefined;
        return new Response('expected websocket', { status: 426 });
      }
      return new Response('not found', { status: 404 });
    },
    websocket: buildWebSocketHandler({
      adapter,
      listenerId,
      processCommand,
      notifyOwnAccountWrite,
      notifyOwnHookWrite,
      notifyOwnCrossTradeWrite,
      onClientConnect,
    }),
  };
}
