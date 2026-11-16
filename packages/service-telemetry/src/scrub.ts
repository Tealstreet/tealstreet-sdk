import { diagnosticText, redactDiagnosticData } from './diagnosticRedaction';

/** String formatting and service payload bounds never define credential policy. */
export const scrubString = diagnosticText;

export function boundServiceData(value: unknown, depth = 0): unknown {
  if (depth > 4) return '[truncated]';
  if (Array.isArray(value)) return value.slice(0, 16).map((item) => boundServiceData(item, depth + 1));
  if (value && typeof value === 'object') {
    return Object.fromEntries(
      Object.entries(value).slice(0, 32).map(([key, child]) => [key, boundServiceData(child, depth + 1)]),
    );
  }
  return value;
}

export function scrub(value: unknown): unknown {
  return boundServiceData(redactDiagnosticData(value));
}
