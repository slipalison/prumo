import { useMutation } from "@tanstack/react-query";
import type { B3Preview } from "./b3ImportTypes";
import { request } from "./client";

/** Envia as planilhas e devolve a prévia. O servidor só lê: nada é gravado. */
export function previewB3Import(files: File[]): Promise<B3Preview> {
  const body = new FormData();
  for (const file of files) body.append("files", file, file.name);
  return request<B3Preview>("POST", "/imports/b3/preview", body);
}

export function useB3Preview() {
  return useMutation({ mutationFn: (files: File[]) => previewB3Import(files) });
}
