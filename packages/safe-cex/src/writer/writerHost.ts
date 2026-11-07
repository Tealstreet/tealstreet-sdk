import type { WriteMarks, WriterCommand, WriterReply, WriterRequest } from './writerRpc';

import { ExchangeError } from '@tealstreet/safe-cex-wasm/host';
import { isWriterReply, isWriterRequest, writerFailure, writerSuccess } from './writerRpc';

/**
 * Where an account's writes run. Each placement (the main thread, the stream
 * worker, a dedicated writer worker, a server process) is a host behind this one
 * interface, so a caller builds the same request whichever it talks to.
 */
export interface WriterHost {
  write(request: WriterRequest): Promise<WriterReply>;
  dispose(): void;
}

/** A writesOnly engine's command entry: the command with its context, answered by the engine. */
export type WriterEngine = (command: WriterCommand & { context: WriterRequest['context'] }) => Promise<unknown>;

const refused = (message: string) => new ExchangeError(message, { message, kind: 'invalidInput', outcome: 'notSent' });

/** The host whose engine runs in this realm. */
export class LocalWriterHost implements WriterHost {
  private disposed = false;

  constructor(
    private readonly account: string,
    private readonly engine: WriterEngine,
  ) {}

  write = async (request: WriterRequest): Promise<WriterReply> => {
    if (this.disposed) return writerFailure(request.id, refused(`${this.account} writer is disposed`), request.marks);
    if (request.account !== this.account) {
      return writerFailure(
        request.id,
        refused(`writer for ${this.account} got a write for ${request.account}`),
        request.marks,
      );
    }
    try {
      const result = await this.engine({ ...request.command, context: request.context });
      return writerSuccess(request.id, result, request.marks);
    } catch (error) {
      return writerFailure(request.id, error, request.marks);
    }
  };

  dispose = () => {
    this.disposed = true;
  };
}

type Pending = {
  resolve: (reply: WriterReply) => void;
  timer: ReturnType<typeof setTimeout>;
  op: string;
  marks?: WriteMarks;
};

/**
 * A host in another realm, reached by message. `send` posts a request and the
 * owner hands every inbound message to `receive`. A reply that never comes is
 * an unknown outcome: the request may have reached the venue.
 */
export class ChannelWriterHost implements WriterHost {
  private readonly pending = new Map<string, Pending>();

  constructor(
    private readonly send: (request: WriterRequest) => void,
    private readonly timeoutMs: number,
  ) {}

  write = (request: WriterRequest): Promise<WriterReply> =>
    new Promise((resolve) => {
      if (this.pending.has(request.id)) {
        resolve(writerFailure(request.id, refused(`duplicate writer request ${request.id}`), request.marks));
        return;
      }
      const timer = setTimeout(() => {
        this.pending.delete(request.id);
        resolve(
          writerFailure(
            request.id,
            interrupted(request.command.op, `writer did not reply within ${this.timeoutMs}ms`),
            request.marks,
          ),
        );
      }, this.timeoutMs);
      this.pending.set(request.id, { resolve, timer, op: request.command.op, marks: request.marks });
      try {
        this.send(request);
      } catch (error) {
        clearTimeout(timer);
        this.pending.delete(request.id);
        const message = error instanceof Error ? error.message : String(error);
        resolve(writerFailure(request.id, refused(`writer request was not delivered: ${message}`), request.marks));
      }
    });

  /** Settles the request a reply answers; false for any other message. */
  receive = (message: unknown): boolean => {
    if (!isWriterReply(message)) return false;
    const pending = this.pending.get(message.id);
    if (!pending) return true;
    clearTimeout(pending.timer);
    this.pending.delete(message.id);
    pending.resolve(
      message.marks || pending.marks ? { ...message, marks: { ...pending.marks, ...message.marks } } : message,
    );
    return true;
  };

  dispose = () => {
    for (const [id, pending] of this.pending) {
      clearTimeout(pending.timer);
      pending.resolve(writerFailure(id, interrupted(pending.op, 'writer channel closed'), pending.marks));
    }
    this.pending.clear();
  };
}

const interrupted = (op: string, message: string) =>
  Object.assign(new Error(message), { code: 'COMMAND_TIMEOUT', command: op, outcome: 'unknown' as const });

/**
 * Serve requests arriving by message from the hosts this realm owns. Returns
 * whether the message was a writer request. A malformed request with an id is
 * refused as not sent; one without an id cannot be answered and is dropped.
 */
export const serveWriter =
  (resolve: (account: string) => WriterHost | undefined, reply: (reply: WriterReply) => void, now: () => number) =>
  (message: unknown): boolean => {
    const kind = (message as { kind?: unknown } | null)?.kind;
    if (kind !== 'writerRequest') return false;
    if (!isWriterRequest(message)) {
      const id = (message as { id?: unknown }).id;
      if (typeof id === 'string' && id) reply(writerFailure(id, refused('malformed writer request')));
      return true;
    }
    const marks = { ...message.marks, received: message.marks?.received ?? now() };
    const host = resolve(message.account);
    if (!host) {
      reply(writerFailure(message.id, refused(`no writer for ${message.account}`), marks));
      return true;
    }
    void host
      .write({ ...message, marks })
      .then(reply, (error: unknown) => reply(writerFailure(message.id, error, marks)));
    return true;
  };
