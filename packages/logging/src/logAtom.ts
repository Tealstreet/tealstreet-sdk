import { atom } from 'jotai';

import { LogSeverity } from './LogSeverity';

export const logAtom = atom(
  [] as {
    timestamp: number;
    message: string;
    severity: LogSeverity;
  }[],
);
