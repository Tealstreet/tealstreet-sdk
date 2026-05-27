import { describe, it, expect } from 'vitest';
import { createPositionId } from './createPositionId';
import { PositionMode, PositionSide } from '../types';

describe('createPositionId', () => {
  it('should return symbol only for one-way mode', () => {
    expect(createPositionId('BTCUSDT', PositionSide.Long, PositionMode.OneWay)).toBe('BTCUSDT');
    expect(createPositionId('BTCUSDT', PositionSide.Short, PositionMode.OneWay)).toBe('BTCUSDT');
  });

  it('should return symbol:side for hedged mode with side', () => {
    expect(createPositionId('BTCUSDT', PositionSide.Long, PositionMode.Hedged)).toBe('BTCUSDT:long');
    expect(createPositionId('BTCUSDT', PositionSide.Short, PositionMode.Hedged)).toBe('BTCUSDT:short');
  });

  it('should return symbol only when no side provided', () => {
    expect(createPositionId('BTCUSDT', undefined, PositionMode.Hedged)).toBe('BTCUSDT');
    expect(createPositionId('BTCUSDT', undefined, PositionMode.OneWay)).toBe('BTCUSDT');
  });

  it('should return symbol:side when mode is undefined but side is provided', () => {
    expect(createPositionId('BTCUSDT', PositionSide.Long, undefined)).toBe('BTCUSDT:long');
    expect(createPositionId('BTCUSDT', PositionSide.Short, undefined)).toBe('BTCUSDT:short');
  });

  it('should return symbol only when both mode and side are undefined', () => {
    expect(createPositionId('BTCUSDT', undefined, undefined)).toBe('BTCUSDT');
  });

  it('should handle various symbol formats', () => {
    expect(createPositionId('ETH-USDT', PositionSide.Long, PositionMode.Hedged)).toBe('ETH-USDT:long');
    expect(createPositionId('BTC/USD', PositionSide.Short, PositionMode.Hedged)).toBe('BTC/USD:short');
  });
});
