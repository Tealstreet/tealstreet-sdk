export interface ChaserOrderState {
  attempts: number;
  isStopped: boolean;
  stopReason: string;
  currentOrdersIds: string[];
}
