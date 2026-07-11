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
let uploadInProgress = false;
let pendingLogBatches: Array<typeof logQueue> = [];

const LOG_FLUSH_INTERVAL_MS = 60_000;
const MAX_RETRIES = 5;
const MAX_RETRY_DELAY_MS = 30_000;
const MAX_PENDING_LOG_BATCHES = 3;
const METRICS_REPORT_INTERVAL_MS = 10 * 60_000;

type LoggingMetrics = {
  generatedEvents: number;
  generatedBytes: number;
  pressureFlushes: number;
  droppedBatches: number;
  uploadsStarted: number;
  uploadsSucceeded: number;
  retries: number;
};

let loggingMetrics: LoggingMetrics = {
  generatedEvents: 0,
  generatedBytes: 0,
  pressureFlushes: 0,
  droppedBatches: 0,
  uploadsStarted: 0,
  uploadsSucceeded: 0,
  retries: 0,
};
let lastMetricsReportAt = Date.now();

type LogIngestToken = {
  expiresAt: number;
  token: string;
};

let cachedLogIngestToken: LogIngestToken | undefined;
let logIngestTokenRequest: Promise<LogIngestToken> | undefined;

const LOG_INGEST_TOKEN_REFRESH_SKEW_MS = 60_000;

export const getSessionKey = () => sessionKey;

// Test-only reset for module-scoped queues. Production code must use the
// lifecycle returned by startLoggingToGrafana instead.
export const resetLoggingStateForTests = () => {
  logQueue = [];
  logQueueSizeApprox = 0;
  pendingLogBatches = [];
  uploadInProgress = false;
  cachedLogIngestToken = undefined;
  logIngestTokenRequest = undefined;
};

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
        loggingMetrics.pressureFlushes++;
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
      loggingMetrics.generatedEvents++;
      loggingMetrics.generatedBytes += Buffer.byteLength(finalMessage, 'utf8');
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
  if (ir.length === 0) return;

  // A noisy source can pressure-flush faster than the network can complete.
  // Keep only a bounded amount of diagnostic data and never fan it out into
  // parallel Vercel function invocations.
  if (pendingLogBatches.length >= MAX_PENDING_LOG_BATCHES) {
    pendingLogBatches.shift();
    loggingMetrics.droppedBatches++;
    console.warn('[Logging] Dropped oldest pending batch because the upload queue is full');
  }
  pendingLogBatches.push(ir);
  void drainLogBatches();
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
      void drainLogBatches();
    }
  };
  const activeOnlineListener = () => void drainLogBatches();
  flushInterval = activeFlushInterval;
  visibilityListener = activeVisibilityListener;

  if (typeof document !== 'undefined') {
    document.addEventListener('visibilitychange', activeVisibilityListener);
    window.addEventListener('online', activeOnlineListener);
  }

  return () => {
    if (flushInterval === activeFlushInterval) {
      clearInterval(activeFlushInterval);
      flushInterval = undefined;
    }
    if (visibilityListener === activeVisibilityListener && typeof document !== 'undefined') {
      document.removeEventListener('visibilitychange', activeVisibilityListener);
      window.removeEventListener('online', activeOnlineListener);
      visibilityListener = undefined;
    }
  };
};

const isPageReadyForLogging = () => {
  const isVisible = typeof document === 'undefined' || document.visibilityState === 'visible';
  const isOnline = typeof navigator === 'undefined' || navigator.onLine !== false;
  return isVisible && isOnline;
};

const getLogIngestOrigin = () => {
  const origin = typeof process !== 'undefined' ? process.env?.NEXT_PUBLIC_LOG_INGEST_ORIGIN : undefined;
  return origin?.replace(/\/+$/, '') || undefined;
};

const getLogIngestToken = async (baseUrl: string) => {
  if (cachedLogIngestToken && cachedLogIngestToken.expiresAt - Date.now() > LOG_INGEST_TOKEN_REFRESH_SKEW_MS) {
    return cachedLogIngestToken.token;
  }

  if (!logIngestTokenRequest) {
    logIngestTokenRequest = fetch(`${baseUrl}/api/logs/token`, { credentials: 'same-origin' })
      .then(async (response) => {
        if (!response.ok) {
          throw new Error(`Log token API responded with status ${response.status}`);
        }

        const payload = (await response.json()) as Partial<LogIngestToken>;
        if (typeof payload.token !== 'string' || typeof payload.expiresAt !== 'number') {
          throw new Error('Log token API returned an invalid response');
        }

        cachedLogIngestToken = { expiresAt: payload.expiresAt, token: payload.token };
        return cachedLogIngestToken;
      })
      .finally(() => {
        logIngestTokenRequest = undefined;
      });
  }

  return (await logIngestTokenRequest).token;
};

const drainLogBatches = async () => {
  if (uploadInProgress) return;
  uploadInProgress = true;
  try {
    while (pendingLogBatches.length > 0) {
      if (!isPageReadyForLogging()) return;
      const batch = pendingLogBatches[0];
      if (batch) {
        const complete = await sendToGrafana(localUserId, batch);
        if (!complete) return;
        pendingLogBatches.shift();
      }
    }
  } finally {
    uploadInProgress = false;
  }
};

const sendToGrafana = async (userId: string, queue: typeof logQueue): Promise<boolean> => {
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
    return true;
  }
  if (queue.length === 0) return true;

  for (let retries = 0; retries <= MAX_RETRIES; retries++) {
    if (!isPageReadyForLogging()) return false;
    const reportMetrics = Date.now() - lastMetricsReportAt >= METRICS_REPORT_INTERVAL_MS;
    const metricsSnapshot = reportMetrics ? { ...loggingMetrics } : undefined;
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
          values: [
            ...queue,
            ...(metricsSnapshot
              ? [
                  [
                    Date.now().toString() + '000000',
                    JSON.stringify({ kind: 'logging_metrics', ...metricsSnapshot }),
                    { level: 'INFO' },
                  ],
                ]
              : []),
          ],
        },
      ],
    };
    try {
      loggingMetrics.uploadsStarted++;
      const baseUrl = typeof self !== 'undefined' && self.location?.origin ? self.location.origin : '';
      const logIngestOrigin = getLogIngestOrigin();
      const headers: Record<string, string> = { 'Content-Type': 'application/json' };
      if (logIngestOrigin) {
        headers.Authorization = `Bearer ${await getLogIngestToken(baseUrl)}`;
      }

      const response = await fetch(logIngestOrigin ? `${logIngestOrigin}/v1/logs` : `${baseUrl}/api/logs`, {
        method: 'POST',
        body: await compress(JSON.stringify(nr)),
        headers,
      });
      if (!response.ok) throw new Error(`Logs API responded with status ${response.status}`);
      loggingMetrics.uploadsSucceeded++;
      if (metricsSnapshot) {
        for (const key of Object.keys(metricsSnapshot) as Array<keyof LoggingMetrics>) {
          loggingMetrics[key] = Math.max(0, loggingMetrics[key] - metricsSnapshot[key]);
        }
        lastMetricsReportAt = Date.now();
      }
      return true;
    } catch (error) {
      if (retries === MAX_RETRIES) {
        console.error(`Error in startLoggingToGrafana: ${error}. Dropping batch after ${MAX_RETRIES} retries`);
        return true;
      }
      const retryDelayMs = Math.min(1_000 * 2 ** retries, MAX_RETRY_DELAY_MS);
      loggingMetrics.retries++;
      console.error(`Error in startLoggingToGrafana: ${error}. Retrying in ${retryDelayMs / 1000}s`);
      await sleep(retryDelayMs);
    }
  }
  return true;
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
