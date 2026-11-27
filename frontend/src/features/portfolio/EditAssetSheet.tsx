import { useId, useState, type Dispatch, type ReactNode, type SetStateAction } from "react";
import { useAssetReferences } from "../../api/ceiling";
import { errorMessage } from "../../api/client";
import { useDeleteAsset, useUpdateAsset } from "../../api/queries";
import type { FundReference, References, StockReference } from "../../api/ceilingTypes";
import type { Asset, ChecklistQuestion, Portfolio, UpdateAssetRequest } from "../../api/types";
import { TAX_INSTRUMENTS, type TaxInstrument } from "../../api/taxTypes";
import { AnswerToggle } from "../../components/AnswerToggle";
import { IconAlert, IconChevronDown, IconSparkle, IconTrash } from "../../components/icons";
import { InfoTip } from "../../components/InfoTip";
import { Grid } from "../../components/Layout";
import { MagneticButton } from "../../components/MagneticButton";
import { MoneyInput } from "../../components/MoneyInput";
import { QuantityInput } from "../../components/QuantityInput";
import { ScoreBadge } from "../../components/ScoreBadge";
import { ScoreSlider } from "../../components/ScoreSlider";
import { Sheet } from "../../components/Sheet";
import { useToast } from "../../components/toastContext";
import { CeilingPicker } from "../ceiling/CeilingPicker";
import { FundamentalsSection } from "../fundamentals/FundamentalsSection";
import { TransactionsSection } from "../ledger/TransactionsSection";
import { INSTRUMENT_HINT, INSTRUMENT_LABEL } from "../tax/taxText";
import { acceptsB3Listing, CLASS_META, unitOf, type UnitKind } from "../../lib/assetClasses";
import {
  choiceOf,
  draftChanges,
  draftOf,
  METHOD_LABEL,
  methodValue,
  referenceFamily,
  type CeilingDraft,
  type ReferenceFamily,
} from "../../lib/ceilingText";
import { formatDay, formatRelative } from "../../lib/format";
import type { Money } from "../../lib/money";
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
  intro?: string;
  /** Chamado depois de salvar de verdade (não quando fecha sem mudança). */
  onSaved?: (asset: Asset) => void;
};

export function EditAssetSheet({ asset, portfolio, onClose, intro, onSaved }: Readonly<EditAssetSheetProps>) {
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

/** Renda fixa por valor: sem preço por unidade, sem teto. */
function valueBasedClass(asset: Asset): boolean {
  return unitOf(asset.asset_class, asset.listed_on_b3, asset.treasury) === "value";
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

type ReferenceItem = StockReference | FundReference;

/** A referência de hoje do ativo (ação ou FII), para a escolha do teto; `null` enquanto carrega ou sem ela. */
function referenceOf(data: References | undefined): ReferenceItem | null {
  if (!data) return null;
  return data.stocks[0] ?? data.funds[0] ?? null;
}

/** O preço de hoje que o motor vai ver: o manual em edição vence a cotação. */
function todayPrice(asset: Asset, useManualPrice: boolean, manualPrice: number): number | null {
  if (useManualPrice) return manualPrice;
  return asset.price_source === "missing" ? null : asset.unit_price;
}

/** O valor do teto em edição: o próprio, o do método pela referência de hoje, ou nenhum. */
function ceilingValue(ceiling: CeilingDraft, item: ReferenceItem | null): number | null {
  if (ceiling.kind === "manual") return ceiling.value;
  if (ceiling.kind === "method" && item) return methodValue(item, ceiling.method).value;
  return null;
}

/** O teto em edição em poucas palavras: "Sem teto", "Graham · R$ 9,50", "Valor próprio · R$ 10,00". */
function ceilingText(ceiling: CeilingDraft, item: ReferenceItem | null, value: number | null, money: Money): string {
  if (ceiling.kind === "none") return "Sem teto";
  if (ceiling.kind === "manual") return `Valor próprio · ${money.brl(ceiling.value)}`;
  if (!item) return METHOD_LABEL[ceiling.method];
  return `${METHOD_LABEL[ceiling.method]} · ${value === null ? "sem valor" : money.brl(value)}`;
}

/** O que está na folha na hora de salvar. */
type EditDraft = {
  listedOnB3: boolean;
  unit: UnitKind;
  valueBased: boolean;
  quantity: number;
  useManualPrice: boolean;
  manualPrice: number;
  ceiling: CeilingDraft;
  manualScoreActive: boolean;
  scoreOverride: number;
  hasChecklist: boolean;
  answers: Record<string, Answer>;
  taxInstrument: TaxInstrument;
};

function setHoldingChange(body: UpdateAssetRequest, asset: Asset, draft: EditDraft): void {
  // O campo de cota inteira mostra o número arredondado; é ele que vai.
  const units = draft.unit === "unit" ? Math.round(draft.quantity) : draft.quantity;
  if (units === asset.quantity) return;
  if (draft.valueBased) body.amount = units;
  else body.quantity = units;
}

function setPriceChanges(body: UpdateAssetRequest, asset: Asset, draft: EditDraft): void {
  if (draft.useManualPrice && draft.manualPrice !== asset.manual_price) {
    body.manual_price = draft.manualPrice;
  } else if (!draft.useManualPrice && asset.manual_price !== null) {
    body.manual_price = null;
  }
  if (draftChanges(draft.ceiling, asset.ceiling)) body.ceiling = choiceOf(draft.ceiling);
}

function setScoreChange(body: UpdateAssetRequest, asset: Asset, draft: EditDraft): void {
  if (draft.manualScoreActive) {
    if (draft.scoreOverride !== asset.score_override) body.score_override = draft.scoreOverride;
  } else if (asset.score_override !== null) {
    body.score_override = null;
  }
}

/** O corpo do `PATCH` só com o que mudou: sem mudança, sai vazio e a folha fecha sem gravar. */
function updateRequest(asset: Asset, draft: EditDraft): UpdateAssetRequest {
  const body: UpdateAssetRequest = {};
  if (draft.listedOnB3 !== asset.listed_on_b3) body.listed_on_b3 = draft.listedOnB3;
  setHoldingChange(body, asset, draft);
  if (!draft.valueBased) setPriceChanges(body, asset, draft);
  setScoreChange(body, asset, draft);
  if (draft.hasChecklist) {
    const diff = answersDiff(asset.answers, draft.answers);
    if (diff) body.answers = diff;
  }
  // Escolher o padrão é voltar a segui-lo (ele muda com a B3 e o nome).
  if (draft.taxInstrument !== asset.tax_instrument) {
    body.tax_instrument = draft.taxInstrument === asset.tax_instrument_default ? null : draft.taxInstrument;
  }
  return body;
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
function SwitchRow({ id, label, tip, checked, onChange }: Readonly<SwitchRowProps>) {
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
function FieldTip({ label, tip, children }: Readonly<{ label: string; tip: ReactNode; children: ReactNode }>) {
  return (
    <div className="field-tip">
      {children}
      <InfoTip label={label} align="end">
        {tip}
      </InfoTip>
    </div>
  );
}

type HoldingFieldProps = {
  ids: { amount: string; quantity: string };
  valueBased: boolean;
  treasury: boolean;
  fractional: boolean;
  value: number;
  onChange: (value: number) => void;
};

/** A quantidade (ou o valor, na renda fixa por valor), com o ⓘ do que mudar aqui grava. */
function HoldingField({ ids, valueBased, treasury, fractional, value, onChange }: Readonly<HoldingFieldProps>) {
  if (valueBased) {
    return (
      <FieldTip label="Sobre o valor atual" tip="Mudar o valor aqui grava uma atualização do saldo em Lançamentos.">
        <MoneyInput id={ids.amount} label="Valor atual" value={value} onChange={onChange} />
      </FieldTip>
    );
  }
  return (
    <FieldTip
      label="Sobre a quantidade"
      tip={
        treasury
          ? "Pode ser fração, como 0,35. Mudar aqui grava um ajuste em Lançamentos; compra e venda, lance lá, com data e preço."
          : "Mudar aqui grava um ajuste em Lançamentos. Compra e venda, lance lá, com data e preço."
      }
    >
      <QuantityInput
        id={ids.quantity}
        label={treasury ? "Quantidade de títulos" : "Quantidade"}
        value={value}
        onChange={onChange}
        fractional={fractional}
      />
    </FieldTip>
  );
}

type PriceFieldsProps = {
  ids: { price: string; priceToggle: string; ceiling: string; ceilingPicker: string };
  asset: Asset;
  name: string;
  useManualPrice: boolean;
  onUseManualPrice: (on: boolean) => void;
  manualPrice: number;
  onManualPrice: (price: number) => void;
  ceiling: CeilingDraft;
  onCeiling: (draft: CeilingDraft) => void;
  ceilingOpen: boolean;
  onToggleCeiling: () => void;
  family: ReferenceFamily | null;
  referenceItem: ReferenceItem | null;
  references: References | undefined;
  referencesLoading: boolean;
};

/**
 * Preço manual e teto lado a lado quando a coluna comporta; o campo do preço
 * e a escolha do teto (a mesma da Análise) abrem embaixo.
 */
function PriceFields({
  ids,
  asset,
  name,
  useManualPrice,
  onUseManualPrice,
  manualPrice,
  onManualPrice,
  ceiling,
  onCeiling,
  ceilingOpen,
  onToggleCeiling,
  family,
  referenceItem,
  references,
  referencesLoading,
}: Readonly<PriceFieldsProps>) {
  const money = useMoney();
  // O preço de hoje que o motor vai ver contra o teto que a pessoa está escolhendo.
  const priceNow = todayPrice(asset, useManualPrice, manualPrice);
  const ceilingNow = ceilingValue(ceiling, referenceItem);
  const aboveNow = ceilingNow !== null && ceilingNow > 0 && priceNow !== null && priceNow > ceilingNow;
  const ceilingSummary = ceilingText(ceiling, referenceItem, ceilingNow, money);
  return (
    <>
      <div className="edit-asset__switches">
        <SwitchRow
          id={ids.priceToggle}
          label="Usar preço manual"
          tip="Vence a cotação automática."
          checked={useManualPrice}
          onChange={onUseManualPrice}
        />
        <div className="edit-asset__ceiling-head">
          <span className="edit-asset__ceiling-title">
            <h3 id={ids.ceiling} className="edit-asset__ceiling-label">
              Teto de preço
            </h3>
            <InfoTip label="Sobre o teto de preço" align="end">
              Seu critério: acima do teto o ativo fica fora do aporte até voltar. Pode seguir um método (o valor muda com o
              balanço, os proventos e as taxas) ou ser um valor seu. Não é preço justo de ninguém.
            </InfoTip>
          </span>
          <button
            type="button"
            className="btn btn--ghost edit-asset__ceiling-toggle"
            aria-expanded={ceilingOpen}
            aria-controls={ids.ceilingPicker}
            aria-label={`Mudar o teto de preço (agora: ${ceilingSummary})`}
            onClick={onToggleCeiling}
          >
            <span>{ceilingSummary}</span>
            <IconChevronDown width={14} height={14} className={ceilingOpen ? "is-open" : undefined} />
          </button>
        </div>
      </div>
      {useManualPrice && <MoneyInput id={ids.price} label="Preço por unidade" value={manualPrice} onChange={onManualPrice} />}
      {ceilingOpen && (
        <fieldset id={ids.ceilingPicker} className="edit-asset__ceiling" aria-labelledby={ids.ceiling}>
          <CeilingPicker
            name={name}
            priceNow={priceNow}
            family={family}
            item={referenceItem}
            market={references?.market ?? null}
            bazinYield={references?.bazin_yield ?? null}
            value={ceiling}
            onChange={onCeiling}
            manualStart={asset.unit_price ?? 0}
            loading={referencesLoading}
          />
        </fieldset>
      )}
      {aboveNow && priceNow !== null && (
        <p className="inline-notice">
          <IconAlert width={16} height={16} style={{ verticalAlign: "-3px" }} /> Hoje está a {money.brl(priceNow)}, acima do teto: fica fora do aporte.
        </p>
      )}
    </>
  );
}

/**
 * A dica do imposto muda com a escolha e diz o que o Prumo faria sozinho: fica
 * à vista, numa linha pequena (a tela de Impostos confere o texto).
 */
function TaxField({
  id,
  asset,
  value,
  onChange,
}: Readonly<{ id: string; asset: Asset; value: TaxInstrument; onChange: (value: TaxInstrument) => void }>) {
  return (
    <div className="field">
      <label htmlFor={id} className="field__label">
        No imposto de renda
      </label>
      <select id={id} className="text-input" value={value} onChange={(event) => onChange(event.target.value as TaxInstrument)}>
        {TAX_INSTRUMENTS.map((option) => (
          <option key={option} value={option}>
            {INSTRUMENT_LABEL[option]}
            {option === asset.tax_instrument_default ? " (padrão)" : ""}
          </option>
        ))}
      </select>
      <span className="field__hint">
        {INSTRUMENT_HINT[value]}{" "}
        {value === asset.tax_instrument_default
          ? "É o padrão do Prumo pela classe, pela B3 e pelo nome."
          : `O padrão do Prumo seria ${INSTRUMENT_LABEL[asset.tax_instrument_default]}.`}
      </span>
    </div>
  );
}

type ScoreFieldsProps = {
  ids: { score: string; manualToggle: string };
  hasChecklist: boolean;
  questions: ChecklistQuestion[];
  answers: Record<string, Answer>;
  onAnswers: Dispatch<SetStateAction<Record<string, Answer>>>;
  useManualScore: boolean;
  onUseManualScore: (on: boolean) => void;
  manualScoreActive: boolean;
  scoreOverride: number;
  onScoreOverride: (score: number) => void;
  liveScore: number;
};

/** O checklist (ou a nota na mão) e o aviso da nota 0. */
function ScoreFields({
  ids,
  hasChecklist,
  questions,
  answers,
  onAnswers,
  useManualScore,
  onUseManualScore,
  manualScoreActive,
  scoreOverride,
  onScoreOverride,
  liveScore,
}: Readonly<ScoreFieldsProps>) {
  return (
    <>
      {hasChecklist && (
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
                  onChange={(value) => onAnswers((current) => ({ ...current, [q.id]: value }))}
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
            onChange={onUseManualScore}
          />
        </section>
      )}
      {manualScoreActive && <ScoreSlider id={ids.score} value={scoreOverride} onChange={onScoreOverride} />}
      {liveScore === 0 && <p className="inline-notice">Com nota 0, este ativo não recebe aporte.</p>}
    </>
  );
}

/** O valor de hoje, o preço com a origem dele e a nota ao vivo. */
function AssetSummary({ asset, valueBased, liveScore }: Readonly<{ asset: Asset; valueBased: boolean; liveScore: number }>) {
  const money = useMoney();
  return (
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
  );
}

type EditAssetFormProps = {
  asset: Asset;
  portfolio: Portfolio;
  intro: string | undefined;
  onDone: () => void;
  onSaved: ((asset: Asset) => void) | undefined;
};

function EditAssetForm({ asset, portfolio, intro, onDone, onSaved }: Readonly<EditAssetFormProps>) {
  const ids = {
    quantity: useId(),
    amount: useId(),
    price: useId(),
    score: useId(),
    manualToggle: useId(),
    priceToggle: useId(),
    b3Toggle: useId(),
    ceiling: useId(),
    ceilingPicker: useId(),
    tax: useId(),
  };
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
  const [ceiling, setCeiling] = useState<CeilingDraft>(() => draftOf(asset.ceiling, asset.unit_price ?? 0));
  // A escolha do teto fica recolhida numa linha (o teto de hoje à vista) e abre no toque.
  const [ceilingOpen, setCeilingOpen] = useState(false);
  // As referências de hoje do ativo (ações e FIIs da B3), para a escolha do teto.
  const family = referenceFamily(asset);
  const references = useAssetReferences(asset.id, family !== null && !valueBasedClass(asset));
  const referenceItem = referenceOf(references.data);
  const [answers, setAnswers] = useState<Record<string, Answer>>({ ...asset.answers });
  const [taxInstrument, setTaxInstrument] = useState<TaxInstrument>(asset.tax_instrument);
  const [confirmDelete, setConfirmDelete] = useState(false);

  const update = useUpdateAsset();
  const remove = useDeleteAsset();
  const toast = useToast();
  const name = assetName(asset);

  const manualScoreActive = useManualScore || meta.checklist === null;
  const liveScore = manualScoreActive ? scoreOverride : checklistScore(answers, questions);

  const save = () => {
    const body = updateRequest(asset, {
      listedOnB3,
      unit,
      valueBased,
      quantity,
      useManualPrice,
      manualPrice,
      ceiling,
      manualScoreActive,
      scoreOverride,
      hasChecklist: meta.checklist !== null,
      answers,
      taxInstrument,
    });
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

  const amountBlock = (
    <>
      {b3Editable && (
        <SwitchRow
          id={ids.b3Toggle}
          label="Negociado na B3"
          tip="ETF ou BDR da B3: cotado em reais pelo ticker, cota inteira."
          checked={listedOnB3}
          onChange={setListedOnB3}
        />
      )}
      <HoldingField
        ids={ids}
        valueBased={valueBased}
        treasury={asset.treasury}
        fractional={unit === "fraction"}
        value={quantity}
        onChange={setQuantity}
      />
    </>
  );

  const priceBlock = !valueBased && (
    <PriceFields
      ids={ids}
      asset={asset}
      name={name}
      useManualPrice={useManualPrice}
      onUseManualPrice={setUseManualPrice}
      manualPrice={manualPrice}
      onManualPrice={setManualPrice}
      ceiling={ceiling}
      onCeiling={setCeiling}
      ceilingOpen={ceilingOpen}
      onToggleCeiling={() => setCeilingOpen((open) => !open)}
      family={family}
      referenceItem={referenceItem}
      references={references.data}
      referencesLoading={references.isPending && family !== null}
    />
  );

  const taxBlock = !valueBased && <TaxField id={ids.tax} asset={asset} value={taxInstrument} onChange={setTaxInstrument} />;

  const scoreBlock = (
    <ScoreFields
      ids={ids}
      hasChecklist={meta.checklist !== null}
      questions={questions}
      answers={answers}
      onAnswers={setAnswers}
      useManualScore={useManualScore}
      onUseManualScore={setUseManualScore}
      manualScoreActive={manualScoreActive}
      scoreOverride={scoreOverride}
      onScoreOverride={setScoreOverride}
      liveScore={liveScore}
    />
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
          <AssetSummary asset={asset} valueBased={valueBased} liveScore={liveScore} />

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
          <fieldset className="danger-zone__confirm" aria-label="Confirmar remoção">
            <span>Remover {name} da carteira?</span>
            <MagneticButton variant="danger" magnetic={false} loading={remove.isPending} onClick={confirmRemove}>
              Remover
            </MagneticButton>
            <MagneticButton variant="ghost" magnetic={false} onClick={() => setConfirmDelete(false)}>
              Cancelar
            </MagneticButton>
          </fieldset>
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
