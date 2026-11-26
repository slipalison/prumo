import { useQueryClient } from "@tanstack/react-query";
import { useId, useState } from "react";
import { analysisKeys } from "../../api/analysis";
import type { Analysis, Reference } from "../../api/analysisTypes";
import { errorMessage } from "../../api/client";
import { useUpdateAsset } from "../../api/queries";
import { AssetIcon } from "../../components/AssetIcon";
import { GlassCard } from "../../components/GlassCard";
import { MagneticButton } from "../../components/MagneticButton";
import { Sheet } from "../../components/Sheet";
import { useToast } from "../../components/toastContext";
import { iconSubjectOfLabel } from "../../lib/assetIcon";
import { formatPercent, plural } from "../../lib/format";
import { useMoney } from "../../lib/privacy";

type References = Analysis["references"];

/**
 * Referências de preço: Bazin (a média de até 5 anos de dividendos por cota
 * dividida pelo rendimento-alvo) e Graham (a raiz de 22,5 vezes o lucro e o
 * patrimônio por ação, dos balanços da CVM). São fórmulas públicas, mostradas
 * como referência: não são preço justo, nem alvo, nem recomendação. Cada uma pode virar o teto de preço do ativo, com
 * confirmação; o resto o Prumo não diz.
 */
export function ReferencesCard({ references }: { references: References }) {
  const titleId = useId();
  const money = useMoney();
  const toast = useToast();
  const client = useQueryClient();
  const update = useUpdateAsset();
  const [pending, setPending] = useState<{ item: Reference; price: number } | null>(null);

  const confirm = () => {
    if (!pending) return;
    update.mutate(
      { id: pending.item.asset_id, body: { max_price: pending.price } },
      {
        onSuccess: () => {
          void client.invalidateQueries({ queryKey: analysisKeys.all });
          toast({ tone: "success", message: `Teto de ${pending.item.label} atualizado.` });
          setPending(null);
        },
        onError: (error) => toast({ tone: "error", message: errorMessage(error) }),
      },
    );
  };

  return (
    <GlassCard as="section" className="references" aria-labelledby={titleId}>
      <h2 id={titleId} className="section-title">
        Referências de preço
      </h2>
      <p className="references__disclaimer" role="note">
        Referência pública, não é recomendação. São fórmulas abertas sobre o histórico de dividendos: não dizem se um
        ativo está barato, caro ou se vale comprar.
      </p>
      <p className="field__hint">
        Bazin: a média anual de dividendos por cota (até 5 anos) dividida pelo seu rendimento-alvo, hoje{" "}
        {formatPercent(references.bazin_yield, Number.isInteger(references.bazin_yield) ? 0 : 2)} ao ano.{" "}
        <a className="link-btn" href="#/ajustes/analise">
          Mudar o rendimento-alvo
        </a>
      </p>
      <p className="field__hint">
        Graham: a raiz de 22,5 vezes o lucro por ação vezes o patrimônio por ação, com os dois positivos (balanços da
        CVM e número de ações da B3). Sem eles, a referência não aparece.
      </p>

      {references.items.length === 0 ? (
        <p className="field__hint">
          Sem referência por enquanto: o Bazin precisa do histórico de dividendos das suas ações e FIIs da B3 e o Graham,
          do lucro e do patrimônio por ação, e o Prumo busca os dois em segundo plano.
        </p>
      ) : (
        <ul className="references__list" aria-label="Referências por ativo">
          {references.items.map((item) => (
            <li key={item.asset_id} className="reference">
              <div className="reference__head">
                <AssetIcon size="sm" assetClass={item.asset_class} {...iconSubjectOfLabel(item.label)} />
                <h3 className="reference__name">{item.label}</h3>
              </div>
              <dl className="reference__numbers">
                {item.bazin && (
                  <div>
                    <dt>Bazin</dt>
                    <dd>{money.brl(item.bazin.price)}</dd>
                  </div>
                )}
                {item.graham !== null && (
                  <div>
                    <dt>Graham</dt>
                    <dd>{money.brl(item.graham)}</dd>
                  </div>
                )}
                <div>
                  <dt>Seu teto</dt>
                  <dd>{item.max_price === null ? "Sem teto" : money.brl(item.max_price)}</dd>
                </div>
                <div>
                  <dt>Preço de hoje</dt>
                  <dd>{item.unit_price === null ? "Sem preço" : money.brl(item.unit_price)}</dd>
                </div>
              </dl>
              {item.bazin && (
                <p className="field__hint">
                  Média de {item.bazin.years} {plural(item.bazin.years, ["ano", "anos"])} de dividendos:{" "}
                  {money.brl(item.bazin.annual_average)} por cota ao ano.
                </p>
              )}
              <div className="reference__actions">
                {item.bazin && item.max_price !== item.bazin.price && (
                  <MagneticButton
                    variant="ghost"
                    magnetic={false}
                    aria-label={`Usar a referência de Bazin como teto de ${item.label}`}
                    onClick={() => item.bazin && setPending({ item, price: item.bazin.price })}
                  >
                    Usar Bazin como meu teto
                  </MagneticButton>
                )}
                {item.graham !== null && item.max_price !== item.graham && (
                  <MagneticButton
                    variant="ghost"
                    magnetic={false}
                    aria-label={`Usar a referência de Graham como teto de ${item.label}`}
                    onClick={() => item.graham !== null && setPending({ item, price: item.graham })}
                  >
                    Usar Graham como meu teto
                  </MagneticButton>
                )}
              </div>
            </li>
          ))}
        </ul>
      )}

      <Sheet
        open={pending !== null}
        onClose={() => setPending(null)}
        title="Usar como teto de preço?"
        description="O teto fica gravado no ativo e vale para o cálculo do aporte."
        footer={
          <>
            <MagneticButton variant="ghost" magnetic={false} onClick={() => setPending(null)}>
              Cancelar
            </MagneticButton>
            <MagneticButton loading={update.isPending} onClick={confirm}>
              Definir teto
            </MagneticButton>
          </>
        }
      >
        {pending && (
          <p>
            Definir {money.brl(pending.price)} como teto de preço de {pending.item.label}? Acima desse preço o ativo
            fica de fora do próximo aporte. Você pode mudar ou tirar o teto quando quiser, ao editar o ativo.
          </p>
        )}
      </Sheet>
    </GlassCard>
  );
}
