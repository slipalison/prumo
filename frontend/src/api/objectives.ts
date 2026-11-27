import { request } from "./client";
import type {
  CreateObjectiveRequest,
  ImpactItem,
  ImpactResponse,
  Objective,
  ObjectivesResponse,
  UpdateObjectiveRequest,
} from "./objectivesTypes";

/**
 * As chaves do cache. Moram aqui (e não em `objectivesQueries`) porque a
 * Renda também invalida os objetivos: a meta de renda é um deles.
 */
export const objectivesKeys = {
  all: ["objectives"] as const,
  list: ["objectives", "list"] as const,
  impact: (items: ImpactItem[]) => ["objectives", "impact", items] as const,
};

export const objectivesApi = {
  list: () => request<ObjectivesResponse>("GET", "/objectives"),
  create: (body: CreateObjectiveRequest) => request<Objective>("POST", "/objectives", body),
  update: (id: string, body: UpdateObjectiveRequest) =>
    request<Objective>("PATCH", `/objectives/${encodeURIComponent(id)}`, body),
  remove: (id: string) => request<undefined>("DELETE", `/objectives/${encodeURIComponent(id)}`),
  impact: (items: ImpactItem[]) => request<ImpactResponse>("POST", "/objectives/impact", { items }),
};
