export type MessageHandlersStats = {
  [topic: string]: {
    lastMessageTimestamp: number;
    lastCallbacksCount?: number; // undefined means N/A
  };
};
export type MessageHandlers = {
  [topic: string]: ((...args: any[]) => void)[];
};
