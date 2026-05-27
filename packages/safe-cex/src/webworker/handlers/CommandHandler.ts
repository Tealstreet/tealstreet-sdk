import { getCommandExecutor } from '../commandExecutor';
import { StoreWorkerPayload } from '../StoreWorkerPayload';
import { BaseHandler } from './BaseHandler';

export class CommandHandler extends BaseHandler {
  handle(payload: StoreWorkerPayload): void {
    switch (payload.op) {
      case 'exec': {
        const executor = getCommandExecutor();
        if (!executor) {
          // No command executor registered for this host (e.g. the
          // exchange-server signing relay never runs CLI commands). Log and
          // no-op rather than crash — safe-cex doesn't own the CLI layer.
          this._exchange.emitter.emit('exec-log', {
            message: 'Command execution is not available in this environment.',
          });
          break;
        }
        // Supports semicolon-chained and @-prefixed parallel commands.
        // payload.data.reqId is optional — when set, the executor emits an
        // `exec-result` event echoing the reqId so the browser-side CLI can
        // match the response to its pending capture-syntax writeback.
        // Fire-and-forget, but handle failure so a thrown command can't
        // surface as an unhandled worker rejection. The try/catch covers a
        // synchronous throw (before a Promise is returned); awaiting inside
        // covers async rejection. (routeCommands already logs per-command
        // failures; this catches a top-level throw.)
        void (async () => {
          try {
            await executor(this._exchange, payload.data.symbol, payload.data.command, payload.data.reqId);
          } catch (err: unknown) {
            this._exchange.emitter.emit('exec-log', {
              message: `Command failed: ${err instanceof Error ? err.message : String(err)}`,
            });
          }
        })();
        break;
      }
    }
  }
}
