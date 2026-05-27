import { useCallback } from 'react';

import { log } from './log';
import { LogSeverity } from './LogSeverity';

export const useLog = () => {
  return useCallback((message: string, severity: LogSeverity = LogSeverity.Info, prefix = '') => {
    log(message, severity, prefix);
  }, []);
};
