import { nanoid } from 'nanoid';

import { compress } from '@tealstreet/utils/src/compression';
import { LogSeverity } from './LogSeverity';

const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

const stringifyMessage = (str: any) => (needsStringification(str) ? JSON.stringify(str) : str);

const needsStringification = (str: any) =>
  typeof str == 'string' ||
  typeof str == 'number' ||
  typeof str == 'boolean' ||
  str === null ||
  str === void 0 ||
  str instanceof Error
    ? false
    : Boolean(Array.isArray(str) || typeof str == 'object');

let logQueue: Array<[string, string, { level: string }]> = [];
let logQueueSizeApprox = 0;
let localUserId = '';
let sessionKey = nanoid();
let flushInterval: ReturnType<typeof setInterval> | undefined;
let visibilityListener: (() => void) | undefined;

const LOG_FLUSH_INTERVAL_MS = 60_000;
const MAX_RETRIES = 5;
const MAX_RETRY_DELAY_MS = 30_000;

export const getSessionKey = () => sessionKey;

function splitStringByBytes(input: string, byteSize: number = 200000): string[] {
  const buffer = Buffer.from(input, 'utf-8');
  const result: string[] = [];
  let start = 0;

  while (start < buffer.length) {
    if (result.length >= 10) {
      // truncate explicitly to avoid endless loop on a client
      return result;
    }
    let end = Math.min(start + byteSize, buffer.length);

    // Ensure not to split multibyte UTF-8 characters
    while (end < buffer.length && (buffer[end] & 0b11000000) === 0b10000000) {
      end--; // Step back to the start of the UTF-8 character
    }

    result.push(buffer.slice(start, end).toString('utf-8'));
    start = end;
  }

  return result;
}

const pushLogsToQueue = (level: LogSeverity, rawMessage: string) => {
  try {
    const date = Date.now().toString() + '000000';
    const messages = splitStringByBytes(rawMessage);
    for (const message of messages) {
      const finalMessage = `${localUserId} [${sessionKey}]: ${stringifyMessage(message)}`;

      if (logQueueSizeApprox + finalMessage.length > 200_000) {
        dumpLogQueue();
      }

      if (finalMessage) {
        logQueue.push([
          date,
          finalMessage,
          {
            level: level === LogSeverity.Error ? 'ERROR' : level === LogSeverity.Warning ? 'WARN' : 'INFO',
          },
        ]);
      }

      logQueueSizeApprox += finalMessage.length;
    }
  } catch (e) {
    // Use console.error to avoid recursive calls
    console.error(`Error in pushLogsToQueue: ${e}`);
  }
};

export const dumpLogQueue = () => {
  const ir = [...logQueue];
  logQueue = [];
  logQueueSizeApprox = 0;
  sendToGrafana(localUserId, ir);
};

export const startLoggingToGrafana = (userId: string) => {
  localUserId = userId;

  if (flushInterval) {
    clearInterval(flushInterval);
  }
  if (visibilityListener && typeof document !== 'undefined') {
    document.removeEventListener('visibilitychange', visibilityListener);
  }

  const flushIfVisible = () => {
    if (typeof document === 'undefined' || document.visibilityState === 'visible') {
      dumpLogQueue();
    }
  };

  const activeFlushInterval = setInterval(flushIfVisible, LOG_FLUSH_INTERVAL_MS);
  const activeVisibilityListener = () => {
    if (document.visibilityState === 'visible') {
      flushIfVisible();
    }
  };
  flushInterval = activeFlushInterval;
  visibilityListener = activeVisibilityListener;

  if (typeof document !== 'undefined') {
    document.addEventListener('visibilitychange', activeVisibilityListener);
  }

  return () => {
    if (flushInterval === activeFlushInterval) {
      clearInterval(activeFlushInterval);
      flushInterval = undefined;
    }
    if (visibilityListener === activeVisibilityListener && typeof document !== 'undefined') {
      document.removeEventListener('visibilitychange', activeVisibilityListener);
      visibilityListener = undefined;
    }
  };
};

const sendToGrafana = async (userId: string, queue: typeof logQueue, retries: number = 0) => {
  // Skip logging in development unless explicitly forced
  // Default to logging if process is undefined (e.g., in web workers)
  let isDevelopment = false;
  let forceLogging = false;

  try {
    isDevelopment = typeof process !== 'undefined' && process.env?.NODE_ENV !== 'production';
    forceLogging = typeof process !== 'undefined' && process.env?.NEXT_PUBLIC_FORCE_EXTERNAL_LOGGING === '1';
  } catch {
    // In environments where process is not defined, default to logging
    console.warn('[Logging] Unable to access process.env, defaulting to enable logging');
  }

  if (isDevelopment && !forceLogging) {
    return;
  }
  if (retries > MAX_RETRIES) {
    // Drop after a bounded number of retries to avoid flooding the logs API.
    return;
  }
  try {
    if (queue.length === 0) return;
    const nr = {
      streams: [
        {
          stream: {
            app: 'tealstreet-terminal-web',
            userId,
            version: '0.0.1',
            sessionKey,
            env: typeof process !== 'undefined' ? process.env?.NODE_ENV : undefined,
          },
          values: [...queue],
        },
      ],
    };
    const baseUrl = typeof self !== 'undefined' && self.location?.origin ? self.location.origin : '';
    // NOTE: body is a gzip ArrayBuffer sent with Content-Type: 'application/json';
    // the /api/logs route decompresses it. That header/body mismatch is intentional.
    // The fetch MUST be awaited and response.ok checked — otherwise a rejected
    // request (offline/connection failure) or a 5xx never reaches the catch below,
    // leaving the retry path dead and dropping the already-cleared batch.
    const response = await fetch(`${baseUrl}/api/logs`, {
      method: 'POST',
      body: await compress(JSON.stringify(nr)),
      headers: {
        'Content-Type': 'application/json',
      },
    });
    if (!response.ok) {
      throw new Error(`Logs API responded with status ${response.status}`);
    }
  } catch (ir) {
    // Use console.error to avoid recursive calls
    const retryDelayMs = Math.min(1_000 * 2 ** retries, MAX_RETRY_DELAY_MS);
    console.error(`Error in startLoggingToGrafana: ${ir}. Retrying in ${retryDelayMs / 1000}s`);
    await sleep(retryDelayMs);
    sendToGrafana(userId, queue, retries + 1);
  }
};

export const logInfo = (...msg: string[]) => {
  try {
    pushLogsToQueue(LogSeverity.Info, msg.join(''));
  } catch (e) {
    // Use console.error to avoid recursive calls
    console.error(`Error in logInfo: ${e}`);
  }
};

export const logWarn = (...msg: string[]) => {
  try {
    pushLogsToQueue(LogSeverity.Warning, msg.join(''));
  } catch (e) {
    // Use console.error to avoid recursive calls
    console.error(`Error in logWarn: ${e}`);
  }
};

export const logError = (...msg: string[]) => {
  try {
    pushLogsToQueue(LogSeverity.Error, msg.join(''));
  } catch (e) {
    // Use console.error to avoid infinite recursion
    console.error(`Error in logError: ${e}`);
  }
};
