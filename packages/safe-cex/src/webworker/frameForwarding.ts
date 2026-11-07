/**
 * Whether the main thread reads engine frames itself (`op: 'frame'`) instead
 * of store payloads, books and candles as objects. The host turns it on through
 * `__workerConfig.acceptsFrames` once its main thread handles frames.
 */
let acceptsFrames = false;

export const setMainAcceptsFrames = (value: boolean) => {
  acceptsFrames = value;
};

export const mainAcceptsFrames = () => acceptsFrames;
