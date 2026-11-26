import { useId, useState } from "react";
import { errorMessage } from "../../api/client";
import { useDeleteAsset, useUpdateAsset } from "../../api/queries";
import type { Asset, Portfolio, UpdateAssetRequest } from "../../api/types";
import { TAX_INSTRUMENTS, type TaxInstrument } from "../../api/taxTypes";
import { AnswerToggle } from "../../components/AnswerToggle";
import { IconAlert, IconSparkle, IconTrash } from "../../components/icons";
import { MagneticButton } from "../../components/MagneticButton";
import { MoneyInput } from "../../components/MoneyInput";
import { QuantityInput } from "../../components/QuantityInput";
import { ScoreBadge } from "../../components/ScoreBadge";
import { ScoreSlider } from "../../components/ScoreSlider";
import { Sheet } from "../../components/Sheet";
import { useToast } from "../../components/toastContext";
import { FundamentalsSection } from "../fundamentals/FundamentalsSection";
import { TransactionsSection } from "../ledger/TransactionsSection";
import { INSTRUMENT_HINT, INSTRUMENT_LABEL } from "../tax/taxText";
import { acceptsB3Listing, CLASS_META, unitOf } from "../../lib/assetClasses";
import { formatDay, formatRelative } from "../../lib/format";
import { useMoney } from "../../lib/privacy";
import { assetName } from "../../lib/rating";
import { answeredCount, answersDiff, checklistScore, type Answer } from "../../lib/score";

type EditAssetSheetProps = {
  asset: Asset | null;
  portfolio: Portfolio;
  onClose: () => void;
  /**
   * Dica curta no topo. Com ela, a folha abre para DAR NOTA: o checklist vem
   * primeiro, antes de quantidade e preço.
   */
  intro?: string | undefined;
  /** Chamado depois de salvar de verdade (não quando fecha sem mudança). */
  onSaved?: ((asset: Asset) => void) | undefined;
};

export function EditAssetSheet({ asset, portfolio, onClose, intro, onSaved }: EditAssetSheetProps) {
  return (
    <Sheet
      open={asset !== null}
      onClose={onClose}
      title={asset ? assetName(asset) : ""}
      description={asset ? sheetDescription(asset) : undefined}
    >
      {asset && (
        <EditAssetForm key={asset.id} asset={asset} portfolio={portfolio} intro={intro} onDone={onClose} onSaved={onSaved} />
      )}
    </Sheet>
  );
}

function sheetDescription(asset: Asset): string {
  const listed = asset.listed_on_b3 && acceptsB3Listing(asset.asset_class) ? " · B3" : "";
  if (asset.treasury) return `${CLASS_META[asset.asset_class].label} · Tesouro Direto`;
  return `${asset.display_name} · ${CLASS_META[asset.asset_class].label}${listed}`;
}

function priceCaption(asset: Asset): string {
  if (asset.price_source === "manual") return "preço manual";
  if (asset.price_source === "missing") return "sem cotação";
  // O Tesouro publica um preço por dia: a data-base diz de quando ele é.
  if (asset.treasury && asset.quote_updated_at) return `preço de resgate de ${formatDay(asset.quote_updated_at)}`;
  return `cotação ${formatRelative(asset.quote_updated_at)}`;
}

type EditAssetFormProps = {
  asset: Asset;
  portfolio: Portfolio;
  intro: string | undefined;
  onDone: () => void;
  onSaved: ((asset: Asset) => void) | undefined;
};

function EditAssetForm({ asset, portfolio, intro, onDone, onSaved }: EditAssetFormProps) {
  const ids = {
    quantity: useId(),
    amount: useId(),
    price: useId(),
    score: useId(),
    manualToggle: useId(),
    priceToggle: useId(),
    b3Toggle: useId(),
    ceiling: useId(),
    ceilingToggle: useId(),
    tax: useId(),
  };
  const money = useMoney();
  const meta = CLASS_META[asset.asset_class];
  const questions = meta.checklist ? portfolio.checklists[meta.checklist] : [];

  const [listedOnB3, setListedOnB3] = useState(asset.listed_on_b3);
  const unit = unitOf(asset.asset_class, listedOnB3, asset.treasury);
  const valueBased = unit === "value";
  // Renda fixa por valor e renda fixa com ticker são cadastros diferentes: a
  // chave só aparece onde trocar de mercado mantém o ativo com ticker.
  const b3Editable = acceptsB3Listing(asset.asset_class) && meta.unit !== "value";

  const [quantity, setQuantity] = useState(asset.quantity);
  // A quantidade é a soma do livro: um lançamento salvo aqui embaixo muda a
  // do servidor, e o campo acompanha — senão "Salvar" gravaria um ajuste
  // desfazendo o lançamento.
  const [serverQuantity, setServerQuantity] = useState(asset.quantity);
  if (serverQuantity !== asset.quantity) {
    setServerQuantity(asset.quantity);
    setQuantity(asset.quantity);
  }
  const [useManualPrice, setUseManualPrice] = useState(asset.manual_price !== null);
  const [manualPrice, setManualPrice] = useState(asset.manual_price ?? asset.unit_price ?? 0);
  const [useManualScore, setUseManualScore] = useState(asset.score_override !== null);
  const [scoreOverride, setScoreOverride] = useState(asset.score_override ?? 5);
  const [useCeiling, setUseCeiling] = useState(asset.max_price !== null);
  const [maxPrice, setMaxPrice] = useState(asset.max_price ?? asset.unit_price ?? 0);
  const [answers, setAnswers] = useState<Record<string, Answer>>({ ...asset.answers });
  const [taxInstrument, setTaxInstrument] = useState<TaxInstrument>(asset.tax_instrument);
  const [confirmDelete, setConfirmDelete] = useState(false);

  const update = useUpdateAsset();
  const remove = useDeleteAsset();
  const toast = useToast();
  const name = assetName(asset);

  const manualScoreActive = useManualScore || meta.checklist === null;
  const liveScore = manualScoreActive ? scoreOverride : checklistScore(answers, questions);

  const buildBody = (): UpdateAssetRequest => {
    const body: UpdateAssetRequest = {};
    if (listedOnB3 !== asset.listed_on_b3) body.listed_on_b3 = listedOnB3;
    // O campo de cota inteira mostra o número arredondado; é ele que vai.
    const units = unit === "unit" ? Math.round(quantity) : quantity;
    if (units !== asset.quantity) {
      if (valueBased) body.amount = units;
      else body.quantity = units;
    }
    if (!valueBased) {
      if (useManualPrice && manualPrice !== asset.manual_price) {
        body.manual_price = manualPrice;
      } else if (!useManualPrice && asset.manual_price !== null) {
        body.manual_price = null;
      }
      if (useCeiling && maxPrice !== asset.max_price) {
        body.max_price = maxPrice;
      } else if (!useCeiling && asset.max_price !== null) {
        body.max_price = null;
      }
    }
    if (manualScoreActive) {
      if (scoreOverride !== asset.score_override) body.score_override = scoreOverride;
    } else if (asset.score_override !== null) {
      body.score_override = null;
    }
    if (meta.checklist) {
      const diff = answersDiff(asset.answers, answers);
      if (diff) body.answers = diff;
    }
    // Escolher o padrão é voltar a segui-lo (ele muda com a B3 e o nome).
    if (taxInstrument !== asset.tax_instrument) {
      body.tax_instrument = taxInstrument === asset.tax_instrument_default ? null : taxInstrument;
    }
    return body;
  };

  const save = () => {
    const body = buildBody();
    if (Object.keys(body).length === 0) {
      onDone();
      return;
    }
    update.mutate(
      { id: asset.id, body },
      {
        onSuccess: (saved) => {
          toast({ tone: "success", message: `${name} atualizado.` });
          onSaved?.(saved);
          onDone();
        },
      },
    );
  };

  const confirmRemove = () => {
    remove.mutate(asset.id, {
      onSuccess: () => {
        toast({ tone: "info", message: `${name} saiu da carteira.` });
        onDone();
      },
    });
  };

  const b3Block = b3Editable && (
    <div className="switch-row">
      <label htmlFor={ids.b3Toggle} className="switch-row__label">
        Negociado na B3
        <span className="field__hint">ETF ou BDR da B3: cotado em reais pelo ticker, cota inteira.</span>
      </label>
      <input
        id={ids.b3Toggle}
        type="checkbox"
        role="switch"
        className="switch"
        checked={listedOnB3}
        onChange={(event) => setListedOnB3(event.target.checked)}
      />
    </div>
  );

  const amountBlock = (
    <>
      {b3Block}
      {valueBased ? (
        <>
          <MoneyInput id={ids.amount} label="Valor atual" value={quantity} onChange={setQuantity} />
          <p className="field__hint">Mudar o valor aqui grava uma atualização do saldo em Lançamentos.</p>
        </>
      ) : (
        <QuantityInput
          id={ids.quantity}
          label={asset.treasury ? "Quantidade de títulos" : "Quantidade"}
          value={quantity}
          onChange={setQuantity}
          fractional={unit === "fraction"}
          hint={
            asset.treasury
              ? "Pode ser fração, como 0,35. Mudar aqui grava um ajuste em Lançamentos; compra e venda, lance lá, com data e preço."
              : "Mudar aqui grava um ajuste em Lançamentos. Compra e venda, lance lá, com data e preço."
          }
        />
      )}
    </>
  );

  const priceBlock = !valueBased && (
    <>
      <div className="switch-row">
        <label htmlFor={ids.priceToggle} className="switch-row__label">
          Usar preço manual
          <span className="field__hint">Vence a cotação automática.</span>
        </label>
        <input
          id={ids.priceToggle}
          type="checkbox"
          role="switch"
          className="switch"
          checked={useManualPrice}
          onChange={(event) => setUseManualPrice(event.target.checked)}
        />
      </div>
      {useManualPrice && <MoneyInput id={ids.price} label="Preço por unidade" value={manualPrice} onChange={setManualPrice} />}
    </>
  );

  // O preço que o motor vai ver (o manual em edição vence a cotação) contra o
  // teto que a pessoa está digitando.
  const priceNow = useManualPrice ? manualPrice : asset.price_source === "missing" ? null : asset.unit_price;
  const aboveNow = useCeiling && maxPrice > 0 && priceNow !== null && priceNow > maxPrice;

  const ceilingBlock = !valueBased && (
    <>
      <div className="switch-row">
        <label htmlFor={ids.ceilingToggle} className="switch-row__label">
          Teto de preço
          <span className="field__hint">
            Seu critério: acima deste preço o ativo fica fora do aporte até voltar. Não é preço justo de ninguém.
          </span>
        </label>
        <input
          id={ids.ceilingToggle}
          type="checkbox"
          role="switch"
          className="switch"
          checked={useCeiling}
          onChange={(event) => setUseCeiling(event.target.checked)}
        />
      </div>
      {useCeiling && (
        <MoneyInput id={ids.ceiling} label="Pagar no máximo, por unidade" value={maxPrice} onChange={setMaxPrice} />
      )}
      {aboveNow && priceNow !== null && (
        <p className="inline-notice">
          <IconAlert width={16} height={16} style={{ verticalAlign: "-3px" }} /> Hoje está a {money.brl(priceNow)}, acima do teto: fica fora do aporte.
        </p>
      )}
    </>
  );

  const taxBlock = !valueBased && (
    <div className="field">
      <label htmlFor={ids.tax} className="field__label">
        No imposto de renda
      </label>
      <select
        id={ids.tax}
        className="text-input"
        value={taxInstrument}
        onChange={(event) => setTaxInstrument(event.target.value as TaxInstrument)}
      >
        {TAX_INSTRUMENTS.map((option) => (
          <option key={option} value={option}>
            {INSTRUMENT_LABEL[option]}
            {option === asset.tax_instrument_default ? " (padrão)" : ""}
          </option>
        ))}
      </select>
      <span className="field__hint">
        {INSTRUMENT_HINT[taxInstrument]}{" "}
        {taxInstrument === asset.tax_instrument_default
          ? "É o padrão do Prumo pela classe, pela B3 e pelo nome."
          : `O padrão do Prumo seria ${INSTRUMENT_LABEL[asset.tax_instrument_default]}.`}
      </span>
    </div>
  );

  const scoreBlock = (
    <>
      {meta.checklist && (
        <section className="edit-asset__checklist" aria-label="Checklist">
          <div className="edit-asset__checklist-head">
            <h3 className="section-title">Checklist</h3>
            <span className="field__hint">
              {answeredCount(answers, questions)} de {questions.length} respondidas
            </span>
          </div>
          {!useManualScore && (
            <div className="answers">
              {questions.map((q) => (
                <AnswerToggle
                  key={q.id}
                  questionId={q.id}
                  prompt={q.prompt}
                  value={answers[q.id] ?? null}
                  onChange={(value) => setAnswers((current) => ({ ...current, [q.id]: value }))}
                />
              ))}
              {questions.length === 0 && <p className="field__hint">Nenhuma pergunta. Crie as suas em Critérios.</p>}
            </div>
          )}
          <div className="switch-row">
            <label htmlFor={ids.manualToggle} className="switch-row__label">
              Definir a nota na mão
              <span className="field__hint">Ignora o checklist para este ativo.</span>
            </label>
            <input
              id={ids.manualToggle}
              type="checkbox"
              role="switch"
              className="switch"
              checked={useManualScore}
              onChange={(event) => setUseManualScore(event.target.checked)}
            />
          </div>
        </section>
      )}
      {manualScoreActive && <ScoreSlider id={ids.score} value={scoreOverride} onChange={setScoreOverride} />}
      {liveScore === 0 && <p className="inline-notice">Com nota 0, este ativo não recebe aporte.</p>}
    </>
  );

  return (
    <div className="edit-asset">
      {intro && (
        <p className="edit-asset__intro">
          <IconSparkle /> {intro}
        </p>
      )}

      <div className="edit-asset__summary">
        <div>
          <span className="eyebrow">Hoje</span>
          <strong className="edit-asset__value">{money.brl(asset.current_value)}</strong>
          {!valueBased && (
            <span className="edit-asset__caption">
              {asset.unit_price !== null && asset.price_source !== "missing" ? money.brl(asset.unit_price) : "—"} ·{" "}
              {priceCaption(asset)}
            </span>
          )}
        </div>
        <ScoreBadge score={liveScore} size="lg" />
      </div>

      {intro ? (
        <>
          {scoreBlock}
          {amountBlock}
          {priceBlock}
          {ceilingBlock}
          {taxBlock}
          <TransactionsSection asset={asset} />
          <FundamentalsSection asset={asset} />
        </>
      ) : (
        <>
          {amountBlock}
          {priceBlock}
          {ceilingBlock}
          {taxBlock}
          <TransactionsSection asset={asset} />
          <FundamentalsSection asset={asset} />
          {scoreBlock}
        </>
      )}

      {(update.isError || remove.isError) && (
        <p className="form-error" role="alert">
          {errorMessage(update.error ?? remove.error)}
        </p>
      )}

      <MagneticButton onClick={save} loading={update.isPending} className="edit-asset__save">
        Salvar
      </MagneticButton>

      <div className="danger-zone">
        {confirmDelete ? (
          <div className="danger-zone__confirm" role="group" aria-label="Confirmar remoção">
            <span>Remover {name} da carteira?</span>
            <MagneticButton variant="danger" magnetic={false} loading={remove.isPending} onClick={confirmRemove}>
              Remover
            </MagneticButton>
            <MagneticButton variant="ghost" magnetic={false} onClick={() => setConfirmDelete(false)}>
              Cancelar
            </MagneticButton>
          </div>
        ) : (
          <button type="button" className="link-btn link-btn--danger" onClick={() => setConfirmDelete(true)}>
            <IconTrash /> Remover da carteira
          </button>
        )}
      </div>
    </div>
  );
}
