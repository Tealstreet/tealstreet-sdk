import { nanoid } from 'nanoid';

import { compress } from '@tealstreet/utils/src/compression';
import { LogSeverity } from './LogSeverity';

const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

const stringifyMessage = (str: unknown) => (needsStringification(str) ? JSON.stringify(str) : str);

const needsStringification = (str: unknown) =>
  typeof str == 'string' ||
  typeof str == 'number' ||
  typeof str == 'boolean' ||
  str === null ||
  str === void 0 ||
  str instanceof Error
    ? false
    : Boolean(Array.isArray(str) || typeof str == 'object');

type LoggingOwner = { userId: string; epoch: number };
type QueuedLogEntry = { owner: LoggingOwner; value: [string, string, { level: string }] };
type LogBatch = { owner: LoggingOwner; entries: QueuedLogEntry[] };

let logQueue: QueuedLogEntry[] = [];
let logQueueSizeApprox = 0;
let localUserId = '';
let localUsername: string | undefined;
let sessionKey = nanoid();
let flushInterval: ReturnType<typeof setInterval> | undefined;
let visibilityListener: (() => void) | undefined;
let onlineListener: (() => void) | undefined;
let uploadInProgress = false;
let pendingLogBatches: LogBatch[] = [];

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

const loggingMetricNames = [
  'generatedEvents',
  'generatedBytes',
  'pressureFlushes',
  'droppedBatches',
  'uploadsStarted',
  'uploadsSucceeded',
  'retries',
] satisfies Array<keyof LoggingMetrics>;

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

type OwnedLogIngestToken = LogIngestToken & LoggingOwner;
let cachedLogIngestToken: OwnedLogIngestToken | undefined;
let logIngestTokenRequest: { userId: string; epoch: number; promise: Promise<LogIngestToken | undefined> } | undefined;
let logIngestTokenEpoch = 0;

const LOG_INGEST_TOKEN_REFRESH_SKEW_MS = 60_000;

function resetLogIngestToken() {
  logIngestTokenEpoch++;
  cachedLogIngestToken = undefined;
  logIngestTokenRequest = undefined;
}

const isLoggingOwnerCurrent = (userId: string, epoch: number) =>
  userId === localUserId && epoch === logIngestTokenEpoch;

const sameLoggingOwner = (left: LoggingOwner, right: LoggingOwner) =>
  left.userId === right.userId && left.epoch === right.epoch;

export const getSessionKey = () => sessionKey;

// Test-only reset for module-scoped queues. Production code must use the
// lifecycle returned by startLoggingToGrafana instead.
export const resetLoggingStateForTests = () => {
  localUserId = '';
  localUsername = undefined;
  logQueue = [];
  logQueueSizeApprox = 0;
  pendingLogBatches = [];
  uploadInProgress = false;
  resetLogIngestToken();
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
      // A handle is diagnostic text, never an unbounded Loki label. Capture it
      // at enqueue so a later rename cannot relabel earlier notices.
      const username = localUsername ? ` username=${JSON.stringify(localUsername)}` : '';
      const finalMessage = `${localUserId}${username} [${sessionKey}]: ${stringifyMessage(message)}`;

      if (logQueueSizeApprox + finalMessage.length > 200_000) {
        loggingMetrics.pressureFlushes++;
        dumpLogQueue();
      }

      if (finalMessage) {
        logQueue.push({
          owner: { userId: localUserId, epoch: logIngestTokenEpoch },
          value: [
            date,
            finalMessage,
            {
              level: level === LogSeverity.Error ? 'ERROR' : level === LogSeverity.Warning ? 'WARN' : 'INFO',
            },
          ],
        });
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

  const batches: LogBatch[] = [];
  for (const entry of ir) {
    const batch = batches.find((candidate) => sameLoggingOwner(candidate.owner, entry.owner));
    if (batch) batch.entries.push(entry);
    else batches.push({ owner: entry.owner, entries: [entry] });
  }
  for (const batch of batches) {
    // Bound pending data while one upload is in progress.
    if (pendingLogBatches.length >= MAX_PENDING_LOG_BATCHES) {
      pendingLogBatches.shift();
      loggingMetrics.droppedBatches++;
      console.warn('[Logging] Dropped oldest pending batch because the upload queue is full');
    }
    pendingLogBatches.push(batch);
  }
  void drainLogBatches();
};

function discardLoggingOwnerWork() {
  // A held token does not authorize deferred work after its lifecycle ends.
  // In-flight uploads also recheck their captured owner after each await.
  logQueue = [];
  logQueueSizeApprox = 0;
  pendingLogBatches = [];
}

export const startLoggingToGrafana = (userId: string, username?: string) => {
  if (userId !== localUserId) {
    discardLoggingOwnerWork();
    resetLogIngestToken();
  }
  localUserId = userId;
  localUsername = username;
  const owner = { userId, epoch: logIngestTokenEpoch };
  const isCurrentOwner = () => isLoggingOwnerCurrent(owner.userId, owner.epoch);

  if (flushInterval) {
    clearInterval(flushInterval);
  }
  if (visibilityListener && typeof document !== 'undefined') {
    document.removeEventListener('visibilitychange', visibilityListener);
  }
  if (onlineListener && typeof window !== 'undefined') {
    window.removeEventListener('online', onlineListener);
  }

  const flushIfVisible = () => {
    if (!isCurrentOwner()) return;
    if (typeof document === 'undefined' || document.visibilityState === 'visible') {
      dumpLogQueue();
    }
  };

  const activeFlushInterval = setInterval(flushIfVisible, LOG_FLUSH_INTERVAL_MS);
  const activeVisibilityListener = () => {
    if (!isCurrentOwner()) return;
    if (document.visibilityState === 'visible') {
      flushIfVisible();
      void drainLogBatches();
    }
  };
  const activeOnlineListener = () => {
    if (isCurrentOwner()) void drainLogBatches();
  };
  flushInterval = activeFlushInterval;
  visibilityListener = activeVisibilityListener;
  onlineListener = activeOnlineListener;

  if (typeof document !== 'undefined') {
    document.addEventListener('visibilitychange', activeVisibilityListener);
    window.addEventListener('online', activeOnlineListener);
  }

  return () => {
    if (flushInterval === activeFlushInterval) {
      clearInterval(activeFlushInterval);
      flushInterval = undefined;
      if (isCurrentOwner()) {
        discardLoggingOwnerWork();
        localUserId = '';
        localUsername = undefined;
        resetLogIngestToken();
      }
    }
    if (visibilityListener === activeVisibilityListener && typeof document !== 'undefined') {
      document.removeEventListener('visibilitychange', activeVisibilityListener);
      visibilityListener = undefined;
    }
    if (onlineListener === activeOnlineListener && typeof window !== 'undefined') {
      window.removeEventListener('online', activeOnlineListener);
      onlineListener = undefined;
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

const getLogIngestToken = async (baseUrl: string, { userId, epoch }: LoggingOwner) => {
  if (!isLoggingOwnerCurrent(userId, epoch)) return undefined;
  if (
    cachedLogIngestToken?.userId === userId &&
    cachedLogIngestToken.epoch === epoch &&
    cachedLogIngestToken.expiresAt - Date.now() > LOG_INGEST_TOKEN_REFRESH_SKEW_MS
  ) {
    return cachedLogIngestToken.token;
  }

  if (logIngestTokenRequest?.userId !== userId || logIngestTokenRequest.epoch !== epoch) {
    const request: Promise<LogIngestToken | undefined> = fetch(`${baseUrl}/api/logs/token`, {
      credentials: 'same-origin',
    })
      .then(async (response) => {
        if (!response.ok) {
          throw new Error(`Log token API responded with status ${response.status}`);
        }

        const payload: unknown = await response.json();
        if (
          !payload ||
          typeof payload !== 'object' ||
          !('token' in payload) ||
          !('expiresAt' in payload) ||
          typeof payload.token !== 'string' ||
          typeof payload.expiresAt !== 'number'
        ) {
          throw new Error('Log token API returned an invalid response');
        }

        if (!isLoggingOwnerCurrent(userId, epoch)) return undefined;
        cachedLogIngestToken = { expiresAt: payload.expiresAt, token: payload.token, userId, epoch };
        return cachedLogIngestToken;
      })
      .finally(() => {
        if (logIngestTokenRequest?.promise === request) logIngestTokenRequest = undefined;
      });
    logIngestTokenRequest = { userId, epoch, promise: request };
  }

  return (await logIngestTokenRequest.promise)?.token;
};

const drainLogBatches = async () => {
  if (uploadInProgress) return;
  const owner = { userId: localUserId, epoch: logIngestTokenEpoch };
  uploadInProgress = true;
  try {
    while (pendingLogBatches.length > 0 && isLoggingOwnerCurrent(owner.userId, owner.epoch)) {
      if (!isPageReadyForLogging()) return;
      const batch = pendingLogBatches[0];
      if (batch) {
        if (!sameLoggingOwner(batch.owner, owner)) {
          pendingLogBatches.shift();
          continue;
        }
        const complete = await sendToGrafana(batch);
        if (!complete) return;
        pendingLogBatches = pendingLogBatches.filter((pending) => pending !== batch);
      }
    }
  } finally {
    uploadInProgress = false;
    if (!isLoggingOwnerCurrent(owner.userId, owner.epoch) && pendingLogBatches.length > 0) {
      void drainLogBatches();
    }
  }
};

const sendToGrafana = async (batch: LogBatch): Promise<boolean> => {
  const { userId, epoch: ownerEpoch } = batch.owner;
  const queue = batch.entries.filter((entry) => sameLoggingOwner(entry.owner, batch.owner)).map((entry) => entry.value);
  const isAuthorized = () => isLoggingOwnerCurrent(userId, ownerEpoch);
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

  const maxRetries = MAX_RETRIES;
  for (let retries = 0; retries <= maxRetries; retries++) {
    if (!isAuthorized()) return true;
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
        const token = await getLogIngestToken(baseUrl, batch.owner);
        if (!token || !isAuthorized()) return true;
        headers.Authorization = `Bearer ${token}`;
      }

      const body = await compress(JSON.stringify(nr));
      if (!isAuthorized()) return true;
      const response = await fetch(logIngestOrigin ? `${logIngestOrigin}/v1/logs` : `${baseUrl}/api/logs`, {
        method: 'POST',
        body,
        headers,
      });
      if (!isAuthorized()) return true;
      if (!response.ok) throw new Error(`Logs API responded with status ${response.status}`);
      if (isLoggingOwnerCurrent(userId, ownerEpoch)) {
        loggingMetrics.uploadsSucceeded++;
        if (metricsSnapshot) {
          for (const key of loggingMetricNames) {
            loggingMetrics[key] = Math.max(0, loggingMetrics[key] - metricsSnapshot[key]);
          }
          lastMetricsReportAt = Date.now();
        }
      }
      return true;
    } catch (error) {
      if (!isAuthorized()) return true;
      if (retries === maxRetries) {
        console.error(`Error in startLoggingToGrafana: ${error}. Dropping batch after ${maxRetries} retries`);
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
