/**
 * Resolve after `ms` milliseconds. See SPEC §2.1 api.utils.sleep.
 */
export const sleep = (ms: number): Promise<void> => new Promise((resolve) => setTimeout(resolve, ms));
