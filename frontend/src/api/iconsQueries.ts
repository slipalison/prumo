import { useQuery } from "@tanstack/react-query";
import { iconsApi } from "./icons";
import type { IconIndex } from "./iconsTypes";

export const iconKeys = {
  index: ["icons"] as const,
};

/** Símbolo → versão do logo, só dos que o servidor tem guardados. */
const byVersion = (index: IconIndex): ReadonlyMap<string, string> =>
  new Map(index.icons.map((icon) => [icon.symbol, icon.version]));

/**
 * Quais símbolos têm logo. Uma consulta para a tela toda: o ícone só pede a
 * imagem do que está na lista, em vez de bater em 404 a cada linha. Logo de um
 * ativo recém-adicionado chega em segundos, e a lista se renova ao voltar para
 * a aba. Erro aqui só quer dizer "sem logos": o ícone cai nas iniciais.
 */
export function useIconIndex() {
  return useQuery({
    queryKey: iconKeys.index,
    queryFn: iconsApi.index,
    select: byVersion,
    staleTime: 60_000,
    retry: false,
  });
}
