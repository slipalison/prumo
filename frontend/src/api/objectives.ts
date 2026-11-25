import { request } from "./client";
import type {
  CreateObjectiveRequest,
  ImpactItem,
  ImpactResponse,
  Objective,
  ObjectivesResponse,
  UpdateObjectiveRequest,
} from "./objectivesTypes";

export const objectivesApi = {
  list: () => request<ObjectivesResponse>("GET", "/objectives"),
  create: (body: CreateObjectiveRequest) => request<Objective>("POST", "/objectives", body),
  update: (id: string, body: UpdateObjectiveRequest) =>
    request<Objective>("PATCH", `/objectives/${encodeURIComponent(id)}`, body),
  remove: (id: string) => request<undefined>("DELETE", `/objectives/${encodeURIComponent(id)}`),
  impact: (items: ImpactItem[]) => request<ImpactResponse>("POST", "/objectives/impact", { items }),
};
