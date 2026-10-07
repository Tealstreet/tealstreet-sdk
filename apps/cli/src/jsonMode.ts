import { reserveStdoutForJson } from './logger';

/**
 * `--json` anywhere in argv puts the process in JSON mode. A quoted `exec`
 * command string is a single argv entry, so `exec 'buy --json'` does not count.
 */
export const isJsonMode = (argv: readonly string[] = process.argv.slice(2)): boolean => argv.includes('--json');

export const enterJsonMode = () => reserveStdoutForJson();

let documentEmitted = false;
let pendingWrite: Promise<void> = Promise.resolve();
let emittedFailed = false;
export const isJsonDocumentEmitted = () => documentEmitted;

/**
 * The one document a `--json` run writes to stdout. Never routed through
 * console, and never twice: a failure in teardown after the result went out
 * must not append a second, contradicting document.
 */
export const emitJson = (value: unknown) =>
  new Promise<void>((resolve) => {
    if (documentEmitted) return resolve();
    documentEmitted = true;
    emittedFailed = (value as { ok?: unknown } | null)?.ok === false;
    pendingWrite = new Promise<void>((done) => process.stdout.write(`${JSON.stringify(value)}\n`, () => done()));
    void pendingWrite.then(resolve);
  });

/**
 * Writes the document, then exits once it is flushed. An agent waits for the
 * process, and a pending background request (the startup check-in has no
 * timeout) would otherwise hold it open after the answer is already out.
 */
export const emitJsonAndExit = async (value: unknown, code = 0): Promise<never> => {
  // A document already out decides the exit: a later crash must not contradict it.
  if (documentEmitted) {
    await pendingWrite;
    process.exit(emittedFailed ? 1 : 0);
  }
  await emitJson(value);
  process.exit(code);
};
