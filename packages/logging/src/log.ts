import { getDefaultStore } from 'jotai';
import slice from 'lodash/slice';

import { logAtom } from './logAtom';
import { LogSeverity } from './LogSeverity';
import { logError, logInfo, logWarn } from './utils';

//import { pinoLogger } from './pinoLogger';

export const log = (message: string, severity: LogSeverity = LogSeverity.Info, prefix = '') => {
  getDefaultStore().set(
    logAtom,
    slice(
      getDefaultStore()
        .get(logAtom)
        .concat({
          timestamp: Date.now(),
          message: `${prefix}${message}`,
          severity,
        }),
      -200,
    ),
  );
  try {
    if (severity === LogSeverity.Error) {
      logError(`${prefix}${message}`);
    } else if (severity === LogSeverity.Warning) {
      logWarn(`${prefix}${message}`);
    } else if (severity === LogSeverity.Info) {
      logInfo(`${prefix}${message}`);
    }
  } catch {}
};

export const grafanaLog = (message: string, severity: LogSeverity = LogSeverity.Info, prefix = '') => {
  try {
    if (severity === LogSeverity.Error) {
      logError(`${prefix}${message}`);
    } else if (severity === LogSeverity.Warning) {
      logWarn(`${prefix}${message}`);
    } else if (severity === LogSeverity.Info) {
      logInfo(`${prefix}${message}`);
    }
  } catch {}
};
