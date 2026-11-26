import { useId } from "react";
import { errorMessage } from "../../api/client";
import type { IncomeAuto } from "../../api/incomeAutoTypes";
import { GlassCard } from "../../components/GlassCard";
import { MagneticButton } from "../../components/MagneticButton";
import { Skeleton } from "../../components/Skeleton";
import { useToast } from "../../components/toastContext";
import { formatDateTime } from "../../lib/format";
import { PendingEventList } from "./PendingEvents";
import { useIncomeAuto, useSaveIncomeAuto } from "./useIncomeAuto";
import "./incomeAuto.css";

/** Ajustes → Proventos automáticos: a chave, o que a B3 cobre e as pendências. */
export function IncomeAutoScreen() {
  const auto = useIncomeAuto();
  return (
    <div className="screen income-auto">
      <header className="screen__header">
        <p className="eyebrow">Renda</p>
        <h1 className="screen__title">Proventos automáticos</h1>
        <p className="screen__lead">
          O Prumo lê na B3 os dividendos, JCP e rendimentos anunciados para os seus ativos e calcula quanto você recebe com
          as cotas do seu livro na data com. O que você lançou à mão ou importou da B3 sempre vence.
        </p>
      </header>

      {auto.isPending && (
        <GlassCard as="section" aria-label="Carregando os proventos automáticos" className="income-auto__card">
          <Skeleton height={28} width="55%" />
          <Skeleton height={64} />
        </GlassCard>
      )}
      {auto.isError && (
        <GlassCard as="section" role="alert" className="income-auto__card">
          <p>{errorMessage(auto.error)}</p>
          <MagneticButton variant="ghost" magnetic={false} onClick={() => void auto.refetch()}>
            Tentar de novo
          </MagneticButton>
        </GlassCard>
      )}
      {auto.data && <Loaded auto={auto.data} />}
    </div>
  );
}

function Loaded({ auto }: { auto: IncomeAuto }) {
  const id = useId();
  const save = useSaveIncomeAuto();
  const toast = useToast();

  const toggle = (enabled: boolean) =>
    save.mutate(enabled, {
      onSuccess: () =>
        toast({
          tone: "success",
          message: enabled
            ? "Proventos automáticos ligados."
            : "Proventos automáticos desligados: os automáticos saíram da Renda.",
        }),
      onError: (error) => toast({ tone: "error", message: errorMessage(error) }),
    });

  return (
    <>
      <GlassCard as="section" className="income-auto__card" aria-labelledby={`${id}-switch-title`}>
        <h2 id={`${id}-switch-title`} className="section-title">
          Calcular pela B3
        </h2>
        <div className="switch-row">
          <span className="switch-row__label">
            <label htmlFor={`${id}-switch`}>Proventos automáticos</label>
            <span className="field__hint">
              Ligado, cada anúncio vira um provento com o selo “automático (B3)”, recebido ou a receber pela data de
              pagamento. Desligado, os automáticos saem da Renda e do caixa; os lançados à mão e os importados ficam.
            </span>
          </span>
          <input
            id={`${id}-switch`}
            type="checkbox"
            role="switch"
            className="switch"
            checked={auto.enabled}
            disabled={save.isPending}
            onChange={(event) => toggle(event.target.checked)}
          />
        </div>
        {auto.enabled && <Coverage auto={auto} />}
      </GlassCard>

      {auto.enabled && (
        <GlassCard as="section" className="income-auto__card" aria-labelledby={`${id}-pending`}>
          <h2 id={`${id}-pending`} className="section-title">
            Eventos em ações
          </h2>
          {auto.pending_events.length === 0 ? (
            <p className="field__hint">
              Nenhuma bonificação, desdobramento ou grupamento pendente. Quando a B3 anunciar um para um ativo seu, ele
              aparece aqui até você lançar no livro.
            </p>
          ) : (
            <PendingEventList events={auto.pending_events} />
          )}
        </GlassCard>
      )}

      <GlassCard as="section" className="income-auto__card" aria-labelledby={`${id}-limits`}>
        <h2 id={`${id}-limits`} className="section-title">
          Como funciona
        </h2>
        <ul className="income-auto__facts">
          <li>A quantidade é a do seu livro no fim da data com: compra na data ex já não dá direito.</li>
          <li>JCP entra com o IR retido pela data de pagamento: 15% até 2025, 17,5% desde 2026.</li>
          <li>Provento com a data de pagamento a definir só entra quando a empresa define a data.</li>
          <li>A B3 mostra de 12 a 18 meses de anúncios; o que for mais antigo continua com você lançando.</li>
          <li>
            Igual a um provento que você já lançou (mesmo ativo, tipo, data e valor por cota a 1%) não duplica. Apagar um
            automático é definitivo: ele não volta.
          </li>
          <li>Rendimento de ETF e de ação entra como “Outro”, sem IR retido: a B3 não informa a retenção.</li>
        </ul>
      </GlassCard>
    </>
  );
}

function Coverage({ auto }: { auto: IncomeAuto }) {
  return (
    <div className="income-auto__coverage">
      <p className="field__hint" role="status">
        {auto.fetched_at
          ? `Anúncios da B3 conferidos em ${formatDateTime(auto.fetched_at)}. A busca é diária.`
          : "A primeira busca na B3 acontece em até uma hora."}
      </p>
      {auto.covered.length > 0 && <p className="field__hint">Acompanhando: {auto.covered.join(", ")}.</p>}
      {auto.unmatched.length > 0 && (
        <p className="field__hint">
          Sem anúncio na B3 (ou código que o Prumo não reconheceu): {auto.unmatched.join(", ")}. Esses continuam só com o
          que você lança.
        </p>
      )}
    </div>
  );
}
