import { request } from "./client";
import type { Analysis, AnalysisConfig, ConfigRequest } from "./analysisTypes";

export const analysisKeys = {
  all: ["analysis"] as const,
  config: ["analysis", "config"] as const,
};

export const analysisApi = {
  analysis: () => request<Analysis>("GET", "/analysis"),
  config: () => request<AnalysisConfig>("GET", "/analysis/rules"),
  saveConfig: (body: ConfigRequest) => request<AnalysisConfig>("PUT", "/analysis/rules", body),
};
