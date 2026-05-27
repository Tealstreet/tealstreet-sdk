import type { ServerWebSocket } from 'bun';
import type { WsSessionData } from '../ws';
import { runWithAdapterContext, type PrintKind } from '../adapter-context';
import { isRemoteExecEnabled } from '../config';
import { logger } from '../../logger';

export type ProcessCommandFn = (command: string) => Promise<void>;

function send(ws: ServerWebSocket<WsSessionData>, frame: Record<string, unknown>): void {
  try {
    ws.send(JSON.stringify(frame));
  } catch (err) {
    logger.warn(`cmd: send failed: ${(err as Error).message}`);
  }
}

export async function handleCmdExec(
  ws: ServerWebSocket<WsSessionData>,
  id: string,
  command: string,
  processCommand: ProcessCommandFn,
  account?: string,
  symbol?: string,
): Promise<void> {
  if (!isRemoteExecEnabled()) {
    send(ws, { t: 'cmd:done', id, success: false, error: 'remoteExec disabled' });
    return;
  }

  const printSink = (kind: PrintKind, text: string) => {
    send(ws, { t: 'cmd:output', id, kind, text });
  };

  try {
    await runWithAdapterContext(
      { printSink, accountOverride: account, symbolOverride: symbol, source: 'cmd-exec' },
      () => processCommand(command),
    );
    send(ws, { t: 'cmd:done', id, success: true });
  } catch (err) {
    send(ws, { t: 'cmd:done', id, success: false, error: (err as Error).message });
  }
}
