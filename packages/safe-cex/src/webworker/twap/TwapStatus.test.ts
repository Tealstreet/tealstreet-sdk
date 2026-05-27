import { describe, it, expect } from 'vitest';
import { TwapStatus } from './TwapStatus';

describe('TwapStatus', () => {
  it('should have Idle status', () => {
    expect(TwapStatus.Idle).toBe('idle');
  });

  it('should have Running status', () => {
    expect(TwapStatus.Running).toBe('running');
  });

  it('should have Paused status', () => {
    expect(TwapStatus.Paused).toBe('paused');
  });

  it('should have Completed status', () => {
    expect(TwapStatus.Completed).toBe('completed');
  });

  it('should have exactly 4 statuses', () => {
    const statuses = Object.values(TwapStatus);
    expect(statuses).toHaveLength(4);
  });

  it('should allow type-safe status assignment', () => {
    const status: TwapStatus = TwapStatus.Running;
    expect(status).toBe('running');
  });
});
