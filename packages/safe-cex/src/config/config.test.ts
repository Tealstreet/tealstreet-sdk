import { describe, it, expect } from 'vitest';
import {
  WS_MAX_RECONNECTION_DELAY,
  WS_MIN_RECONNECTION_DELAY,
  RECONNECTING_WEBSOCKET_TUNING,
  WS_BACKOFF_BASE_DELAY,
  WS_BACKOFF_MAX_DELAY,
  WS_BACKOFF_FACTOR,
  WS_FAILURE_STREAK_PAUSE_THRESHOLD,
  WS_FAILURE_STREAK_PAUSE_MS,
  ENABLE_RECONNECTING_WEBSOCKET,
  ENABLE_PWS_LOG,
} from './config';

describe('config', () => {
  describe('WebSocket constants', () => {
    it('should have WS_MAX_RECONNECTION_DELAY as 5000ms', () => {
      expect(WS_MAX_RECONNECTION_DELAY).toBe(5000);
    });

    it('should have WS_MIN_RECONNECTION_DELAY between 1000 and 3000ms', () => {
      expect(WS_MIN_RECONNECTION_DELAY).toBeGreaterThanOrEqual(1000);
      expect(WS_MIN_RECONNECTION_DELAY).toBeLessThan(3000);
    });

    it('should have RECONNECTING_WEBSOCKET_TUNING with correct values', () => {
      expect(RECONNECTING_WEBSOCKET_TUNING.maxReconnectionDelay).toBe(WS_MAX_RECONNECTION_DELAY);
      expect(RECONNECTING_WEBSOCKET_TUNING.minReconnectionDelay).toBe(WS_MIN_RECONNECTION_DELAY);
    });
  });

  describe('Backoff constants', () => {
    it('should have WS_BACKOFF_BASE_DELAY as 2000ms', () => {
      expect(WS_BACKOFF_BASE_DELAY).toBe(2000);
    });

    it('should have WS_BACKOFF_MAX_DELAY as 30000ms', () => {
      expect(WS_BACKOFF_MAX_DELAY).toBe(30000);
    });

    it('should have WS_BACKOFF_FACTOR as 2', () => {
      expect(WS_BACKOFF_FACTOR).toBe(2);
    });

    it('should have WS_FAILURE_STREAK_PAUSE_THRESHOLD as 8', () => {
      expect(WS_FAILURE_STREAK_PAUSE_THRESHOLD).toBe(8);
    });

    it('should have WS_FAILURE_STREAK_PAUSE_MS as 120000ms', () => {
      expect(WS_FAILURE_STREAK_PAUSE_MS).toBe(120000);
    });
  });

  describe('Feature flags', () => {
    it('should have ENABLE_RECONNECTING_WEBSOCKET as boolean', () => {
      expect(typeof ENABLE_RECONNECTING_WEBSOCKET).toBe('boolean');
    });

    it('should disable raw private WebSocket logging by default', () => {
      expect(ENABLE_PWS_LOG).toBe(false);
    });
  });

  describe('Config relationships', () => {
    it('should have min delay less than max delay', () => {
      expect(WS_MIN_RECONNECTION_DELAY).toBeLessThan(WS_MAX_RECONNECTION_DELAY);
    });

    it('should have backoff base delay less than max delay', () => {
      expect(WS_BACKOFF_BASE_DELAY).toBeLessThan(WS_BACKOFF_MAX_DELAY);
    });

    it('should allow reasonable exponential backoff progression', () => {
      // With base 2000 and factor 2, after 4 retries: 2000 -> 4000 -> 8000 -> 16000 -> 32000
      // 32000 > 30000 so it caps at max
      const maxRetries = Math.ceil(Math.log(WS_BACKOFF_MAX_DELAY / WS_BACKOFF_BASE_DELAY) / Math.log(WS_BACKOFF_FACTOR));
      expect(maxRetries).toBeGreaterThan(0);
      expect(maxRetries).toBeLessThan(10); // Should reach max in reasonable retries
    });

    it('should have pause window longer than max backoff delay', () => {
      expect(WS_FAILURE_STREAK_PAUSE_MS).toBeGreaterThan(WS_BACKOFF_MAX_DELAY);
    });
  });
});
