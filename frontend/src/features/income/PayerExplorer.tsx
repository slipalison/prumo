import { motion } from "motion/react";
import { useEffect, useMemo, useRef, useState } from "react";
import type { IncomePayer } from "../../api/incomeTypes";
import { AssetIcon } from "../../components/AssetIcon";
import { IconArrowRight, IconCheck } from "../../components/icons";
import { Panel } from "../../components/Layout";
import { iconSubjectOfLabel } from "../../lib/assetIcon";
import { cx } from "../../lib/cx";
import { formatPercent } from "../../lib/format";
import { OTHERS_KEY, payerSlices, type PayerSlice } from "../../lib/income";
import { enterUp } from "../../lib/motion";
import { useMoney } from "../../lib/privacy";
import { yieldText } from "../../lib/yieldOnCost";
import { FilterChip } from "./FilterChip";
import { PayerDonut } from "./PayerDonut";
import { ScrollRegion } from "./ScrollRegion";

type PayerExplorerProps = {
  payers: IncomePayer[];
  total: number;
  /** A chave do pagador que filtra as listas de pagamento; `null` sem filtro. */
  selected: string | null;
  onSelect: (key: string | null) => void;
};

/**
 * "Quem pagou": a rosca e a tabela dos 12 meses para explorar.
 *
 * - Clicar (ou Enter) num pagador, na tabela ou na fatia, o destaca e filtra as
 *   listas de pagamento; clicar de novo, ou no "x" do selo no cabeçalho, limpa.
 * - Clicar em "Outros" **entra no grupo**: a rosca passa a mostrar só quem está
 *   lá dentro e a tabela vira a lista dos pagadores do grupo, com o "Voltar a
 *   todos os pagadores" no topo. A lista longa ocupa o espaço que antes sobrava
 *   no painel, e o foco vai para o "Voltar" (e volta para "Outros" ao sair).
 * - Passar o mouse, focar ou tocar numa fatia, ou numa linha, mostra o nome e o
 *   valor no meio da rosca e acende os dois. Com o olho fechado, todo R$ (o do
 *   meio, o do nome acessível) sai mascarado.
 */
export function PayerExplorer({ payers, total, selected, onSelect }: Readonly<PayerExplorerProps>) {
  const money = useMoney();
  const slices = useMemo(() => payerSlices(payers), [payers]);
  const others = slices.find((s) => s.key === OTHERS_KEY) ?? null;
  const [inside, setInside] = useState(false);
  const [hover, setHover] = useState<string | null>(null);
  const back = useRef<HTMLButtonElement>(null);
  const othersRow = useRef<HTMLButtonElement>(null);
  const wasInside = useRef(inside);
  const scroller = useRef<HTMLDivElement>(null);

  const group = inside && others ? others : null;
  const view = group ? group.members : slices;
  const viewTotal = group ? group.total : total;
  const selectedSlice = selected ? slices.concat(others?.members ?? []).find((s) => s.key === selected) ?? null : null;

  // O arco ou a linha da tela (e não de outra): o que o ponteiro toca vale mais que o filtro.
  const activeKey = hover ?? (selected && view.some((s) => s.key === selected) ? selected : null);
  const active = view.find((s) => s.key === activeKey) ?? null;

  // O botão em que a pessoa clicou some da tela: o foco acompanha para onde ela foi.
  useEffect(() => {
    if (wasInside.current === inside) return;
    wasInside.current = inside;
    (inside ? back.current : othersRow.current)?.focus();
  }, [inside]);

  /** A fatia acesa pelo ponteiro ou pelo foco traz a linha dela para a janela da tabela (só a tabela rola). */
  const lightUp = (key: string | null) => {
    setHover(key);
    const box = scroller.current;
    const row = key ? Array.from(box?.querySelectorAll<HTMLElement>("tbody tr") ?? []).find((r) => r.dataset.key === key) : null;
    if (!box || !row) return;
    if (row.offsetTop < box.scrollTop) box.scrollTop = row.offsetTop;
    else if (row.offsetTop + row.offsetHeight > box.scrollTop + box.clientHeight) box.scrollTop = row.offsetTop + row.offsetHeight - box.clientHeight;
  };

  const enter = () => {
    setHover(null);
    setInside(true);
  };
  const leave = () => {
    setHover(null);
    // O filtro de quem está dentro de "Outros" não tem onde aparecer fora do grupo.
    if (selected && others?.members.some((s) => s.key === selected)) onSelect(null);
    setInside(false);
  };
  const pick = (key: string) => {
    if (key === OTHERS_KEY) enter();
    else onSelect(selected === key ? null : key);
  };

  const percentOf = (slice: PayerSlice) => `${formatPercent(slice.percent)} da renda`;
  const arcLabel = (slice: PayerSlice) =>
    slice.key === OTHERS_KEY
      ? `${slice.label}: ${money.brl(slice.total)}, ${percentOf(slice)}. Ver quem está no grupo`
      : `${slice.label}: ${money.brl(slice.total)}, ${percentOf(slice)}`;

  const inGroup = group ? ` dentro de ${group.label}` : "";
  const shares = view.map((s) => `${s.label} ${formatPercent(s.percent)}`).join(", ");
  const donutLabel = `Renda dos últimos 12 meses por pagador${inGroup}: ${shares}`;

  return (
    <Panel
      title="Quem pagou"
      className="payers-panel"
      info="O que cada pagador somou nos últimos 12 meses e o yield on cost dele (a renda ÷ o que você pagou). Clique num pagador para filtrar os pagamentos ao lado; clique em Outros para ver quem está lá."
      infoLabel="Como explorar Quem pagou"
      actions={
        <>
          {group && (
            <button ref={back} type="button" className="link-btn payers__back" onClick={leave}>
              <IconArrowRight aria-hidden="true" width={16} height={16} className="payers__back-icon" />
              Voltar<span className="payers__back-long"> a todos os pagadores</span>
            </button>
          )}
          {selectedSlice && <FilterChip label={selectedSlice.label} onClear={() => onSelect(null)} />}
        </>
      }
    >
      <motion.div key={group ? "inside" : "all"} className="payers" {...enterUp}>
        <PayerDonut
          arcs={view.map((s) => ({
            key: s.key,
            value: s.total,
            color: s.color,
            label: arcLabel(s),
            pressed: s.key === OTHERS_KEY ? undefined : selected === s.key,
          }))}
          label={donutLabel}
          activeKey={activeKey}
          onActive={lightUp}
          onPick={pick}
        >
          {active ? (
            <>
              <span className="donut__caption payers__center-name">{active.label}</span>
              <strong className="donut__number donut__number--sm">{money.brl(active.total)}</strong>
              <span className="donut__caption">{formatPercent(active.percent)}</span>
            </>
          ) : (
            <>
              <span className="donut__caption">{group ? group.label : "12 meses"}</span>
              <strong className="donut__number donut__number--sm">{money.compact(viewTotal)}</strong>
            </>
          )}
        </PayerDonut>
        <ScrollRegion scrollRef={scroller} className="payers__scroll" label={group ? `Quem pagou dentro de ${group.label}, lista com rolagem` : "Quem pagou, lista com rolagem"}>
          <table className="payers__table">
            <caption className="sr-only">{group ? `Quem pagou dentro de ${group.label}` : "Quem pagou nos últimos 12 meses"}</caption>
            <thead>
              <tr>
                <th scope="col">Pagador</th>
                <th scope="col" className="is-end">
                  Recebido
                </th>
                <th scope="col" className="is-end">
                  % da renda
                </th>
                <th scope="col" className="is-end">
                  Yield on cost
                </th>
              </tr>
            </thead>
            <tbody>
              {view.map((slice) => {
                const isOthers = slice.key === OTHERS_KEY;
                const isSelected = selected === slice.key;
                return (
                  <tr
                    key={slice.key}
                    data-key={slice.key}
                    className={cx(isSelected && "is-selected", slice.key === activeKey && "is-active")}
                    onPointerEnter={() => setHover(slice.key)}
                    onPointerLeave={() => setHover(null)}
                  >
                    <th scope="row">
                      <button
                        ref={isOthers ? othersRow : undefined}
                        type="button"
                        className="payers__pick"
                        aria-pressed={isOthers ? undefined : isSelected}
                        onClick={() => pick(slice.key)}
                        onFocus={() => setHover(slice.key)}
                        onBlur={() => setHover(null)}
                      >
                        <span className="dot" style={{ background: slice.color }} aria-hidden="true" />
                        {!isOthers && <AssetIcon size="sm" {...iconSubjectOfLabel(slice.label)} />}
                        <span className="payers__name">{slice.label}</span>
                        {isOthers && (
                          <>
                            <span className="sr-only"> — ver quem está no grupo</span>
                            <IconArrowRight aria-hidden="true" width={16} height={16} className="payers__more" />
                          </>
                        )}
                        {isSelected && <IconCheck aria-hidden="true" width={14} height={14} className="payers__check" />}
                      </button>
                    </th>
                    <td className="is-end payers__value">
                      <strong>{money.brl(slice.total)}</strong>
                    </td>
                    <td className="is-end muted payers__share">
                      {formatPercent(slice.percent)}
                    </td>
                    <td className="is-end payers__yield">
                      {slice.yield && (
                        <>
                          <span className="payers__yield-label" aria-hidden="true">
                            yield on cost{" "}
                          </span>
                          <strong>{yieldText(slice.yield)}</strong>
                        </>
                      )}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </ScrollRegion>
      </motion.div>
    </Panel>
  );
}
