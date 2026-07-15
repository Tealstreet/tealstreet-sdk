export const wooV3CancelAllAfterFixtures = Object.freeze({
  disableTimerResponse: Object.freeze({
    success: true,
    timestamp: 1_725_000_000_000,
    data: Object.freeze({ expectedTriggerTime: 0 }),
  }),
  scheduledResponse: Object.freeze({
    success: true,
    timestamp: 1_725_000_000_000,
    data: Object.freeze({ expectedTriggerTime: 1_725_000_900_000 }),
  }),
});
