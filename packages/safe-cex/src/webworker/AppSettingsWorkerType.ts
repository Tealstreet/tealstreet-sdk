import { OrderStopTriggerType } from '../types';

export type AppSettingsWorkerType = {
  placeOrder: {
    tpTriggerType: OrderStopTriggerType;
    slTriggerType: OrderStopTriggerType;
  };
};
