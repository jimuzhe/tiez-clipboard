export interface UpdateModalData {
  version: string;
  notes: string;
  downloadUrl: string;
}

export interface AiProfile {
  id: string;
  baseUrl: string;
  apiKey: string;
  model: string;
  enableThinking: boolean;
}

export type AppCleanupPolicyAction = "ignore" | "clean";

export interface AppCleanupPolicy {
  id: string;
  enabled: boolean;
  appName: string;
  appPath: string;
  action: AppCleanupPolicyAction;
  contentTypes: string[];
  cleanupRules: string;
}

export type EditableAiProfile = Omit<AiProfile, "id"> & { id?: string; isNew?: boolean };

export type AiProfileStatus = "loading" | "success" | "error" | "none";

export type AiProfileStatusMap = Record<string, AiProfileStatus>;
