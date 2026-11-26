import { useId, useState, type ReactNode } from "react";
import { errorMessage } from "../../api/client";
import { useDeleteAsset, useUpdateAsset } from "../../api/queries";
import type { Asset, Portfolio, UpdateAssetRequest } from "../../api/types";
import { TAX_INSTRUMENTS, type TaxInstrument } from "../../api/taxTypes";
import { AnswerToggle } from "../../components/AnswerToggle";
import { IconAlert, IconSparkle, IconTrash } from "../../components/icons";
import { InfoTip } from "../../components/InfoTip";
import { Grid } from "../../components/Layout";
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
      size="wide"
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

type SwitchRowProps = {
  id: string;
  label: string;
  /** A explicação de uma frase, sob demanda (ⓘ ao lado do nome). */
  tip?: ReactNode;
  checked: boolean;
  onChange: (checked: boolean) => void;
};

/** Interruptor de uma linha: o nome, o ⓘ com o porquê e a chave à direita. */
function SwitchRow({ id, label, tip, checked, onChange }: SwitchRowProps) {
  return (
    <div className="switch-row edit-asset__switch">
      <span className="edit-asset__switch-label">
        <label htmlFor={id} className="switch-row__label">
          {label}
        </label>
        {tip && (
          <InfoTip label={`Sobre ${label.toLowerCase()}`} align="start">
            {tip}
          </InfoTip>
        )}
      </span>
      <input
        id={id}
        type="checkbox"
        role="switch"
        className="switch"
        checked={checked}
        onChange={(event) => onChange(event.target.checked)}
      />
    </div>
  );
}

/** Um campo com o ⓘ na linha do rótulo (canto direito), no lugar da dica sob o campo. */
function FieldTip({ label, tip, children }: { label: string; tip: ReactNode; children: ReactNode }) {
  return (
    <div className="field-tip">
      {children}
      <InfoTip label={label} align="end">
        {tip}
      </InfoTip>
    </div>
  );
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
    <SwitchRow
      id={ids.b3Toggle}
      label="Negociado na B3"
      tip="ETF ou BDR da B3: cotado em reais pelo ticker, cota inteira."
      checked={listedOnB3}
      onChange={setListedOnB3}
    />
  );

  const amountBlock = (
    <>
      {b3Block}
      {valueBased ? (
        <FieldTip label="Sobre o valor atual" tip="Mudar o valor aqui grava uma atualização do saldo em Lançamentos.">
          <MoneyInput id={ids.amount} label="Valor atual" value={quantity} onChange={setQuantity} />
        </FieldTip>
      ) : (
        <FieldTip
          label="Sobre a quantidade"
          tip={
            asset.treasury
              ? "Pode ser fração, como 0,35. Mudar aqui grava um ajuste em Lançamentos; compra e venda, lance lá, com data e preço."
              : "Mudar aqui grava um ajuste em Lançamentos. Compra e venda, lance lá, com data e preço."
          }
        >
          <QuantityInput
            id={ids.quantity}
            label={asset.treasury ? "Quantidade de títulos" : "Quantidade"}
            value={quantity}
            onChange={setQuantity}
            fractional={unit === "fraction"}
          />
        </FieldTip>
      )}
    </>
  );

  // O preço de hoje que o motor vai ver (o manual em edição vence a cotação)
  // contra o teto que a pessoa está digitando.
  const priceNow = useManualPrice ? manualPrice : asset.price_source === "missing" ? null : asset.unit_price;
  const aboveNow = useCeiling && maxPrice > 0 && priceNow !== null && priceNow > maxPrice;

  // Preço manual e teto: dois interruptores lado a lado quando a coluna comporta;
  // o campo de cada um abre embaixo.
  const priceBlock = !valueBased && (
    <>
      <div className="edit-asset__switches">
        <SwitchRow
          id={ids.priceToggle}
          label="Usar preço manual"
          tip="Vence a cotação automática."
          checked={useManualPrice}
          onChange={setUseManualPrice}
        />
        <SwitchRow
          id={ids.ceilingToggle}
          label="Teto de preço"
          tip="Seu critério: acima deste preço o ativo fica fora do aporte até voltar. Não é preço justo de ninguém."
          checked={useCeiling}
          onChange={setUseCeiling}
        />
      </div>
      {useManualPrice && <MoneyInput id={ids.price} label="Preço por unidade" value={manualPrice} onChange={setManualPrice} />}
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

  // A dica do imposto muda com a escolha e diz o que o Prumo faria sozinho: fica
  // à vista, numa linha pequena (a tela de Impostos confere o texto).
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
          <SwitchRow
            id={ids.manualToggle}
            label="Definir a nota na mão"
            tip="Ignora o checklist para este ativo."
            checked={useManualScore}
            onChange={setUseManualScore}
          />
        </section>
      )}
      {manualScoreActive && <ScoreSlider id={ids.score} value={scoreOverride} onChange={setScoreOverride} />}
      {liveScore === 0 && <p className="inline-notice">Com nota 0, este ativo não recebe aporte.</p>}
    </>
  );

  const fieldsBlock = (
    <>
      {amountBlock}
      {priceBlock}
      {taxBlock}
    </>
  );

  return (
    <div className="edit-asset">
      {intro && (
        <p className="edit-asset__intro">
          <IconSparkle /> {intro}
        </p>
      )}

      <Grid cols={2} alignStart className="edit-asset__cols">
        <div className="edit-asset__col">
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
              {fieldsBlock}
            </>
          ) : (
            <>
              {fieldsBlock}
              {scoreBlock}
            </>
          )}
        </div>

        <div className="edit-asset__col">
          <TransactionsSection asset={asset} />
          <FundamentalsSection asset={asset} />
        </div>
      </Grid>

      {(update.isError || remove.isError) && (
        <p className="form-error" role="alert">
          {errorMessage(update.error ?? remove.error)}
        </p>
      )}

      <div className="edit-asset__bar">
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
        <MagneticButton onClick={save} loading={update.isPending} className="edit-asset__save">
          Salvar
        </MagneticButton>
      </div>
    </div>
  );
}
