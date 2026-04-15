/// <reference types="vite/client" />

interface ImportMetaEnv {
  readonly VITE_EDITION?: "local" | "cloud";
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
