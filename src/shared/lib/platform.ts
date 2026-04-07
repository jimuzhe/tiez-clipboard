export const isMacPlatform = (): boolean => {
  if (typeof navigator === "undefined") return false;

  return (
    /Mac|iPhone|iPad|iPod/i.test(navigator.userAgent) ||
    /Mac/i.test(navigator.platform)
  );
};
