type CompactPreviewControls = {
  forceHide: () => void;
  warmup: () => void;
  supported: () => boolean;
  warmupSupported: () => boolean;
};

let controls: CompactPreviewControls | null = null;

export const registerCompactPreviewControls = (next: CompactPreviewControls) => {
  controls = next;
};

export const forceHideCompactPreviewWindow = () => {
  controls?.forceHide();
};

export const warmupCompactPreviewWindow = () => {
  controls?.warmup();
};

export const isCompactPreviewWindowSupported = () =>
  controls?.supported() ?? false;

export const isCompactPreviewWarmupSupported = () =>
  controls?.warmupSupported() ?? false;
