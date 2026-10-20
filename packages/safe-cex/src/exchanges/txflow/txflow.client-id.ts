import type { ClientOrderIdRule } from '../clientOrderId';
import { uuid } from '../../utils/uuid';

export const validTxFlowCloid = (id: string): boolean =>
  id.length > 0 && new TextEncoder().encode(id).length <= 64 && !id.startsWith('0x');

export const txflowClientOrderIdRule: ClientOrderIdRule = {
  label: 'TxFlow',
  requirement: 'be nonempty, at most 64 UTF-8 bytes and not start with 0x',
  isValid: validTxFlowCloid,
  mint: () => `ts_${uuid()}`,
};
