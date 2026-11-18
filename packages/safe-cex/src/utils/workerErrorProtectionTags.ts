/** Closed safety fact: native Error properties do not survive worker structured cloning. */
export const workerErrorProtectionTags = (value: unknown): { worker_error_unprotected?: true } => {
  try {
    if (value !== null && typeof value === 'object' && 'protection' in value && value.protection === 'unprotected') {
      return { worker_error_unprotected: true };
    }
  } catch {
    // Diagnostics must not introduce another failure from a foreign error getter.
  }
  return {};
};
