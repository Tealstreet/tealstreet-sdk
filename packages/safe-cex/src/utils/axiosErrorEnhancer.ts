import { AxiosError } from 'axios';

export interface EnhancedAxiosError extends Error {
  isAxiosError: boolean;
  code?: string;
  request?: any;
  response?: any;
  config?: any;
  toJSON?: () => object;
  originalError?: AxiosError;
  enhancedMessage?: string;
  context?: {
    url?: string;
    method?: string;
    statusCode?: number;
    statusText?: string;
    errorCode?: string;
    isNetworkError?: boolean;
    isTimeout?: boolean;
    isCORS?: boolean;
    timestamp?: string;
  };
}

/**
 * Enhances axios error messages with more context for better debugging and Sentry tracking
 */
export function enhanceAxiosError(error: AxiosError, exchangeName?: string): EnhancedAxiosError {
  let enhancedError = error as EnhancedAxiosError;

  // if somehow error is not an error
  if (typeof error !== 'object' || !(error instanceof Error)) {
    enhancedError = new Error(error) as EnhancedAxiosError;
    enhancedError.isAxiosError = true;
  } else if (!enhancedError.isAxiosError) {
    enhancedError.isAxiosError = true;
  }

  // Build context object
  const context: EnhancedAxiosError['context'] = {
    timestamp: new Date().toISOString(),
  };

  // Extract URL and method from config
  if (error.config) {
    context.url = error.config.url;
    context.method = error.config.method?.toUpperCase();
  }

  // Only set properties if enhancedError is an object
  if (typeof enhancedError === 'object' && enhancedError !== null) {
    // Check if it's a network error
    if (!error.response && error.request) {
      context.isNetworkError = true;

      // Check specific error codes
      if (error.code === 'ECONNABORTED') {
        context.isTimeout = true;
        enhancedError.enhancedMessage = `Request timeout${context.url ? ` to ${context.url}` : ''}`;
      } else if (error.code === 'ERR_NETWORK' || error.message === 'Network Error') {
        if (
          typeof window !== 'undefined' &&
          typeof XMLHttpRequest !== 'undefined' &&
          error.request instanceof XMLHttpRequest
        ) {
          context.isCORS = true;
          enhancedError.enhancedMessage = `Network error (possibly CORS)${context.url ? ` when accessing ${context.url}` : ''}`;
        } else {
          enhancedError.enhancedMessage = `Network connection failed${context.url ? ` to ${context.url}` : ''}`;
        }
      } else if (error.code) {
        context.errorCode = error.code;
        enhancedError.enhancedMessage = `Network error (${error.code})${context.url ? ` to ${context.url}` : ''}`;
      } else {
        enhancedError.enhancedMessage = `Network request failed${context.url ? ` to ${context.url}` : ''}`;
      }
    } else if (error.response) {
      // Server responded with error
      context.statusCode = error.response.status;
      context.statusText = error.response.statusText;

      const data = error.response.data as any;
      const serverMessage =
        data?.msg ||
        data?.message ||
        data?.error ||
        data?.retMsg || // Bybit format
        data?.code; // Some exchanges use 'code' field

      enhancedError.enhancedMessage = `${context.method || 'Request'} ${context.url || 'to API'} failed with ${context.statusCode} ${context.statusText}${serverMessage ? `: ${serverMessage}` : ''}`;
    } else {
      // Error in request setup
      enhancedError.enhancedMessage = `Request setup error${context.url ? ` for ${context.url}` : ''}: ${error.message}`;
    }

    // Add exchange name to message if provided
    if (exchangeName) {
      enhancedError.enhancedMessage = `[${exchangeName}] ${enhancedError.enhancedMessage}`;
    }
  }

  // Store context only if enhancedError is an object
  if (typeof enhancedError === 'object' && enhancedError !== null) {
    enhancedError.context = context;
    enhancedError.originalError = error;

    // Override the message property
    Object.defineProperty(enhancedError, 'message', {
      get() {
        return this.enhancedMessage || error.message;
      },
      enumerable: true,
      configurable: true,
    });
  }

  return enhancedError;
}

/**
 * Checks if an error is a network error based on axios error properties
 */
export function isNetworkError(error: any): boolean {
  if (!error || !error.isAxiosError) {
    return false;
  }

  return (
    !error.response &&
    Boolean(error.code) &&
    error.code !== 'ECONNABORTED' &&
    (error.code === 'ERR_NETWORK' ||
      error.code === 'ETIMEDOUT' ||
      error.code === 'ENOTFOUND' ||
      error.code === 'ECONNREFUSED' ||
      error.code === 'ECONNRESET' ||
      error.message === 'Network Error')
  );
}

/**
 * Formats error context for Sentry
 */
export function formatErrorForSentry(error: EnhancedAxiosError) {
  return {
    tags: {
      errorType: error.context?.isNetworkError ? 'network' : 'api',
      errorCode: error.context?.errorCode || error.code,
      statusCode: error.context?.statusCode,
      isTimeout: error.context?.isTimeout,
      isCORS: error.context?.isCORS,
    },
    extra: {
      url: error.context?.url,
      method: error.context?.method,
      statusText: error.context?.statusText,
      enhancedMessage: error.enhancedMessage,
      originalMessage: error.originalError?.message,
      timestamp: error.context?.timestamp,
      config: error.config
        ? {
            baseURL: error.config.baseURL,
            timeout: error.config.timeout,
            headers: error.config.headers ? Object.keys(error.config.headers) : undefined,
          }
        : undefined,
    },
  };
}
