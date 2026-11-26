import { useMutation, useQueryClient } from "@tanstack/react-query";
import type { B3CommitResult, B3Decisions, B3PreviewResponse } from "./b3ImportTypes";
import { cashKeys } from "./cashQueries";
import { request } from "./client";
import { healthKeys } from "./health";
import { incomeKeys } from "./incomeQueries";
import { ledgerKeys } from "./ledger";
import { keys } from "./queries";

export type B3Upload = { files: File[]; decisions: B3Decisions };

/**
 * O envio: as planilhas e, numa parte `decisions` (JSON), as escolhas da
 * pessoa. Os arquivos ficam só na memória do navegador; o servidor não guarda
 * nada entre a prévia e a gravação, então o "Importar" manda tudo de novo.
 */
function form(files: File[], decisions: B3Decisions & { digest?: string }): FormData {
  const body = new FormData();
  body.append("decisions", JSON.stringify(decisions));
  for (const file of files) body.append("files", file, file.name);
  return body;
}

/** A prévia com o plano: o que é novo, o que já existe e o que conflita. Nada é gravado. */
export function previewB3Import({ files, decisions }: B3Upload): Promise<B3PreviewResponse> {
  return request<B3PreviewResponse>("POST", "/imports/b3/preview", form(files, decisions));
}

/** Grava o plano conferido (o `digest` da prévia), tudo ou nada. */
export function commitB3Import({ files, decisions, digest }: B3Upload & { digest: string }): Promise<B3CommitResult> {
  return request<B3CommitResult>("POST", "/imports/b3/commit", form(files, { ...decisions, digest }));
}

export function useB3Preview() {
  return useMutation({ mutationFn: previewB3Import });
}

/**
 * Depois de gravar, tudo o que lê a carteira, o livro, a renda, o caixa e o
 * desempenho é lido de novo (a série do desempenho se refaz no servidor: a tela
 * mostra o aviso de reconstrução enquanto isso).
 */
export function useB3Commit() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: commitB3Import,
    onSuccess: () =>
      Promise.all(
        [keys.portfolio, ledgerKeys.all, incomeKeys.all, cashKeys.all, healthKeys.all, ["net-worth"], ["performance"]].map(
          (queryKey) => client.invalidateQueries({ queryKey }),
        ),
      ),
  });
}
