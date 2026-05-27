import { OrderStopTriggerType } from '../types';

export type ExchangeMetadataWorkerType = {
  supportedTPSLTriggerTypes: OrderStopTriggerType[];
  privateProxyIps: string[];
  defaultTPTriggerType: OrderStopTriggerType | undefined;
  defaultSLTriggerType: OrderStopTriggerType | undefined;
  fetchLeverageOnMarginSymbolChange?: boolean;
};
