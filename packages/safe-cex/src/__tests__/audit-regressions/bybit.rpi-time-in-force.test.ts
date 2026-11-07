import { describe, expect, it } from 'vitest';

import {
  ORDER_TIME_IN_FORCE as DERIVATIVES_TIME_IN_FORCE,
  ORDER_TIME_IN_FORCE_OUTBOUND as DERIVATIVES_TIME_IN_FORCE_OUTBOUND,
} from '../../exchanges/bybit/bybit.types';
import {
  ORDER_TIME_IN_FORCE as SPOT_TIME_IN_FORCE,
  ORDER_TIME_IN_FORCE_OUTBOUND as SPOT_TIME_IN_FORCE_OUTBOUND,
} from '../reference/bybit/bybitspot.types';
import { OrderTimeInForce } from '../../types';

describe('Bybit RPI order mapping', () => {
  it('renders externally placed derivatives and spot RPI orders as maker-only orders', () => {
    expect(DERIVATIVES_TIME_IN_FORCE.RPI).toBe(OrderTimeInForce.PostOnly);
    expect(SPOT_TIME_IN_FORCE.RPI).toBe(OrderTimeInForce.PostOnly);
    expect(DERIVATIVES_TIME_IN_FORCE_OUTBOUND[OrderTimeInForce.PostOnly]).toBe('PostOnly');
    expect(SPOT_TIME_IN_FORCE_OUTBOUND[OrderTimeInForce.PostOnly]).toBe('PostOnly');
  });
});
