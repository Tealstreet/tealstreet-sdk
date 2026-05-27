import { describe, it, expect } from 'vitest';
import { AxiosError } from 'axios';
import { enhanceAxiosError, isNetworkError } from './axiosErrorEnhancer';

describe('axiosErrorEnhancer', () => {
  describe('enhanceAxiosError', () => {
    it('should enhance network error with context', () => {
      const error = new AxiosError('Network Error', 'ERR_NETWORK', {
        url: '/api/v1/orders',
        method: 'GET',
        headers: {},
      } as any, {});
      
      const enhanced = enhanceAxiosError(error, 'binance');
      
      expect(enhanced.enhancedMessage).toContain('[binance]');
      expect(enhanced.enhancedMessage).toContain('Network connection failed');
      expect(enhanced.enhancedMessage).toContain('/api/v1/orders');
      expect(enhanced.context?.isNetworkError).toBe(true);
      expect(enhanced.context?.url).toBe('/api/v1/orders');
      expect(enhanced.context?.method).toBe('GET');
    });

    it('should enhance timeout error', () => {
      const error = new AxiosError('timeout of 5000ms exceeded', 'ECONNABORTED', {
        url: '/api/v1/account',
        method: 'POST',
        headers: {},
      } as any, {});
      
      const enhanced = enhanceAxiosError(error, 'okx');
      
      expect(enhanced.enhancedMessage).toContain('[okx]');
      expect(enhanced.enhancedMessage).toContain('Request timeout');
      expect(enhanced.context?.isTimeout).toBe(true);
    });

    it('should enhance server error with response data', () => {
      const error = new AxiosError('Request failed with status code 400', 'ERR_BAD_REQUEST', {
        url: '/api/v1/order',
        method: 'POST',
        headers: {},
      } as any, null, {
        status: 400,
        statusText: 'Bad Request',
        data: { msg: 'Invalid symbol' },
        headers: {},
        config: {} as any,
      });
      
      const enhanced = enhanceAxiosError(error);
      
      expect(enhanced.enhancedMessage).toContain('POST /api/v1/order failed with 400 Bad Request');
      expect(enhanced.enhancedMessage).toContain('Invalid symbol');
      expect(enhanced.context?.statusCode).toBe(400);
      expect(enhanced.context?.statusText).toBe('Bad Request');
    });

    it('should handle CORS error indication', () => {
      // Mock window and XMLHttpRequest
      (global as any).window = {};
      class MockXHR {}
      (global as any).XMLHttpRequest = MockXHR as any;
      const mockXHR = new MockXHR() as XMLHttpRequest;
      
      const error = new AxiosError('Network Error', 'ERR_NETWORK', {
        url: 'https://api.exchange.com/v1/data',
        method: 'GET',
        headers: {},
      } as any, mockXHR);
      
      const enhanced = enhanceAxiosError(error);
      
      expect(enhanced.enhancedMessage).toContain('Network error (possibly CORS)');
      expect(enhanced.context?.isCORS).toBe(true);
      
      // Clean up
      delete (global as any).window;
      delete (global as any).XMLHttpRequest;
    });
  });

  describe('isNetworkError', () => {
    it('should detect network errors correctly', () => {
      const networkError = new AxiosError('Network Error', 'ERR_NETWORK', { headers: {} } as any, {});
      expect(isNetworkError(networkError)).toBe(true);
      
      const timeoutError = new AxiosError('timeout', 'ECONNABORTED', { headers: {} } as any, {});
      expect(isNetworkError(timeoutError)).toBe(false);
      
      const serverError = new AxiosError('Server Error', undefined, { headers: {} } as any, null, {
        status: 500,
        statusText: 'Internal Server Error',
        data: {},
        headers: {},
        config: {} as any,
      });
      expect(isNetworkError(serverError)).toBe(false);
    });
  });
});
