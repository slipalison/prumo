import { request } from "./client";
import type { Cash } from "./cashTypes";

export const cashApi = {
  cash: () => request<Cash>("GET", "/cash"),
};
