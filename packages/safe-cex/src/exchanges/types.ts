export type MessageHandlersStats = {
  [topic: string]: {
    lastMessageTimestamp: number;
    lastCallbacksCount?: number; // undefined means N/A
    /**
     * True once this topic was seen with a live `messageHandlers` entry. It is
     * what separates "handler-backed and now torn down" (must stop being
     * reported) from "never handler-backed" (pong, bare ticker — must stay
     * reported). Absent handlers alone cannot make that distinction: several
     * exchanges `delete` the key on teardown.
     */
    hadHandlers?: boolean;
  };
};
export type MessageHandlers = {
  [topic: string]: ((...args: any[]) => void)[];
};
