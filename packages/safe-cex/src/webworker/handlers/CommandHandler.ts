import { serializeCommandError } from '../../commandOutcome';
import { getCommandExecutor } from '../commandExecutor';
import { StoreWorkerPayload } from '../StoreWorkerPayload';
import { BaseHandler } from './BaseHandler';

export class CommandHandler extends BaseHandler {
  handle(payload: StoreWorkerPayload): void {
    switch (payload.op) {
      case 'exec': {
        if (this.runInRustAlgos(payload)) break;
        const data = payload.data && typeof payload.data === 'object' ? payload.data : undefined;
        const reqId = data?.reqId;
        if (!data) {
          const message = 'Invalid command payload.';
          this._exchange.emitter.emit('exec-log', reqId === undefined ? { message } : { message, reqId });
          if (reqId !== undefined) {
            this._exchange.emitter.emit('exec-result', { reqId, handled: false, success: false, error: message });
          }
          break;
        }

        const executor = getCommandExecutor();
        if (!executor) {
          // No command executor registered for this host. Answer the reqId
          // rather than crash — safe-cex doesn't own the CLI layer.
          const message = 'Command execution is not available in this environment.';
          this._exchange.emitter.emit('exec-log', reqId === undefined ? { message } : { message, reqId });
          if (reqId !== undefined) {
            this._exchange.emitter.emit('exec-result', { reqId, handled: false, success: false, error: message });
          }
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
            await executor(this._exchange, data.symbol, data.command, reqId);
          } catch (err: unknown) {
            const message = `Command failed: ${err instanceof Error ? err.message : String(err)}`;
            this._exchange.emitter.emit('exec-log', reqId === undefined ? { message } : { message, reqId });
            // A caller awaiting this reqId would otherwise wait out its timeout.
            if (reqId !== undefined) {
              this._exchange.emitter.emit('exec-result', {
                reqId,
                handled: true,
                success: false,
                error: message,
                ...(err instanceof Error && { errorDetails: serializeCommandError(err) }),
              });
            }
          }
        })();
        break;
      }
    }
  }
}
