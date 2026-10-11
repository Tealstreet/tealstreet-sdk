import type { AxiosError, AxiosRequestConfig } from 'axios';

import { isNetworkError } from 'axios-retry';

export type IsReadRequest = (config: AxiosRequestConfig) => boolean;

export const isReadOnlyMethod: IsReadRequest = (config) => {
  const method = config.method?.toLowerCase();
  return method === 'get' || method === 'head';
};

// A write whose response was lost may already be live at the venue, and
// resending it places the order twice. Only reads are ever retried.
export const onlyReads =
  (shouldRetry: (error: AxiosError) => boolean, isRead: IsReadRequest = isReadOnlyMethod) =>
  (error: AxiosError): boolean =>
    error.config !== undefined && isRead(error.config) && shouldRetry(error);

export const networkErrorOnRead = (isRead: IsReadRequest = isReadOnlyMethod) => onlyReads(isNetworkError, isRead);

// For venues that POST a few reads: GET/HEAD plus those paths, compared without the query.
export const readsAndPosted =
  (...paths: string[]): IsReadRequest =>
  (config) =>
    isReadOnlyMethod(config) || paths.includes(config.url?.split('?')[0] ?? '');
