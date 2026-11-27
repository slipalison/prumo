import { useEffect, useId, useState, type FormEvent } from "react";
import { errorMessage } from "../../api/client";
import { useCreateIncome, useEntitlement } from "../../api/incomeQueries";
import { INCOME_KINDS, type CreateIncomeRequest, type Entitlement, type IncomeKind } from "../../api/incomeTypes";
import type { Asset } from "../../api/types";
import { MagneticButton } from "../../components/MagneticButton";
import { Sheet } from "../../components/Sheet";
import { useToast } from "../../components/toastContext";
import { unitOf } from "../../lib/assetClasses";
import { cx } from "../../lib/cx";
import { parseDecimal } from "../../lib/format";
import { InfoTip } from "../../components/InfoTip";
import { formatIsoDate, isUsListed, KIND_LABEL, previewAmounts, type Amounts } from "../../lib/income";
import { useMoney } from "../../lib/privacy";

type AddIncomeSheetProps = {
  open: boolean;
  onClose: () => void;
  assets: Asset[];
  /** Hoje (`AAAA-MM-DD`), a data de pagamento sugerida. */
  today: string;
  /** Ativo já escolhido (ao lançar a partir de uma linha). */
  initialAssetId?: string;
};

const label = (asset: Asset) => asset.symbol ?? asset.display_name;

/** O tipo mais provável para a classe do ativo: FII rende, o resto paga dividendo. */
function defaultKind(asset: Asset | undefined): IncomeKind {
  return asset && (asset.asset_class === "br_reits" || asset.asset_class === "us_reits") ? "reit_income" : "dividend";
}

/** O formulário como a pessoa o deixou, mais o que o livro diz da data com. */
type Draft = {
  asset: Asset | undefined;
  kind: IncomeKind;
  payDate: string;
  exDate: string;
  perUnit: string;
  quantity: string;
  /** `null` = a pessoa não mexeu: vale o padrão do tipo e do mercado. */
  withholdingText: string | null;
  /** As cotas do livro na data com (1 no ativo lançado por valor); `null` enquanto o servidor não diz. */
  bookQuantity: number | null;
  usListed: boolean;
};

/** O que o formulário vale agora: os avisos, a prévia e o lançamento pronto para o servidor. */
type DraftCheck = {
  noSharesOnRecordDate: boolean;
  /** A prévia com o imposto padrão: o que o campo do imposto mostra enquanto a pessoa não mexe nele. */
  defaultPreview: Amounts | null;
  preview: Amounts | null;
  withholdingAboveGross: boolean;
  /** O lançamento; `null` enquanto falta um campo ou um deles não fecha. */
  request: CreateIncomeRequest | null;
};

/** O corpo do `POST /income`: quantidade, data ex e imposto só quando a pessoa os informou. */
function incomeRequest(
  asset: Asset,
  kind: IncomeKind,
  payDate: string,
  exDate: string,
  unitValue: number,
  typedQuantity: number | null,
  typedWithholding: number | null,
): CreateIncomeRequest {
  return {
    asset_id: asset.id,
    kind,
    pay_date: payDate,
    amount_per_unit: unitValue,
    ...(typedQuantity !== null ? { quantity: typedQuantity } : {}),
    ...(exDate ? { ex_date: exDate } : {}),
    ...(typedWithholding !== null ? { withholding: typedWithholding } : {}),
  };
}

/** Confere o formulário e faz a prévia de bruto, imposto retido e líquido. */
function checkDraft({ asset, kind, payDate, exDate, perUnit, quantity, withholdingText, bookQuantity, usListed }: Draft): DraftCheck {
  const unitValue = parseDecimal(perUnit);
  const typedQuantity = parseDecimal(quantity);
  const effectiveQuantity = typedQuantity ?? bookQuantity;
  const noSharesOnRecordDate = typedQuantity === null && bookQuantity !== null && bookQuantity <= 0;
  const typedWithholding = withholdingText === null ? null : parseDecimal(withholdingText);
  const withholdingInvalid = typedWithholding !== null && typedWithholding < 0;
  const valid =
    asset !== undefined &&
    unitValue !== null &&
    unitValue > 0 &&
    !noSharesOnRecordDate &&
    (typedQuantity === null || typedQuantity > 0) &&
    !withholdingInvalid &&
    payDate !== "" &&
    (exDate === "" || exDate <= payDate);
  const defaultPreview =
    valid && effectiveQuantity !== null ? previewAmounts(kind, unitValue, effectiveQuantity, payDate, undefined, usListed) : null;
  const preview =
    valid && effectiveQuantity !== null
      ? previewAmounts(kind, unitValue, effectiveQuantity, payDate, typedWithholding ?? undefined, usListed)
      : null;
  return {
    noSharesOnRecordDate,
    defaultPreview,
    preview,
    withholdingAboveGross: preview !== null && preview.withholding > preview.gross,
    request: valid ? incomeRequest(asset, kind, payDate, exDate, unitValue, typedQuantity, typedWithholding) : null,
  };
}

/**
 * Lançar um provento à mão: ativo, tipo, data de pagamento, valor por cota e
 * quantidade. A prévia mostra bruto, imposto retido e líquido (JCP perde 15%
 * na fonte até 2025 e 17,5% desde 2026); o servidor refaz a conta.
 */
export function AddIncomeSheet({ open, onClose, assets, today, initialAssetId }: Readonly<AddIncomeSheetProps>) {
  const ids = { withholding: useId(), asset: useId(), pay: useId(), ex: useId(), unit: useId(), quantity: useId() };
  const [assetId, setAssetId] = useState(initialAssetId ?? assets[0]?.id ?? "");
  const asset = assets.find((a) => a.id === assetId);
  const byValue = asset ? unitOf(asset.asset_class, asset.listed_on_b3) === "value" : false;
  const [kind, setKind] = useState<IncomeKind>(defaultKind(asset));
  const [payDate, setPayDate] = useState(today);
  const [exDate, setExDate] = useState("");
  const [perUnit, setPerUnit] = useState("");
  const [quantity, setQuantity] = useState("");
  // `null` = a pessoa não mexeu: vale o padrão do tipo e do mercado.
  const [withholdingText, setWithholdingText] = useState<string | null>(null);
  const create = useCreateIncome();
  // As cotas que o livro tinha na data com: é o que vale com a quantidade em
  // branco, e o servidor usa a mesma regra.
  const entitlement = useEntitlement(assetId, exDate, payDate, open && !byValue);
  const toast = useToast();

  // Cada abertura começa limpa.
  useEffect(() => {
    if (!open) return;
    const first = assets.find((a) => a.id === initialAssetId) ?? assets[0];
    setAssetId(first?.id ?? "");
    setKind(defaultKind(first));
    setPayDate(today);
    setExDate("");
    setPerUnit("");
    setQuantity("");
    setWithholdingText(null);
    create.reset();
    // `create` muda de identidade a cada render; só a abertura recomeça o formulário.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);

  const chooseAsset = (id: string) => {
    setAssetId(id);
    setKind(defaultKind(assets.find((a) => a.id === id)));
    setQuantity("");
  };

  // Com a quantidade em branco, vale a do livro na data com — se o servidor
  // ainda não disse qual é, a prévia espera, e quem decide é ele ao lançar.
  const bookQuantity = byValue ? 1 : (entitlement.data?.quantity ?? null);
  const usListed = asset ? isUsListed(asset) : false;
  const { noSharesOnRecordDate, defaultPreview, preview, withholdingAboveGross, request } = checkDraft({
    asset,
    kind,
    payDate,
    exDate,
    perUnit,
    quantity,
    withholdingText,
    bookQuantity,
    usListed,
  });

  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (!request || withholdingAboveGross || !asset || create.isPending) return;
    create.mutate(request, {
      onSuccess: () => {
        toast({ tone: "success", message: `Provento de ${label(asset)} lançado.` });
        onClose();
      },
    });
  };

  return (
    <Sheet
      open={open}
      onClose={onClose}
      title="Lançar provento"
      description="Dividendo, JCP, rendimento de FII ou amortização que você recebeu (ou vai receber)."
    >
      {assets.length === 0 ? (
        <p className="field__hint">Cadastre um ativo na Carteira primeiro: o provento é sempre de um ativo.</p>
      ) : (
        <form className="asset-form" onSubmit={submit} noValidate>
          <div className="field">
            <label htmlFor={ids.asset} className="field__label">
              Ativo
            </label>
            <select id={ids.asset} className="text-input" value={assetId} onChange={(event) => chooseAsset(event.target.value)}>
              {assets.map((a) => (
                <option key={a.id} value={a.id}>
                  {label(a)}
                </option>
              ))}
            </select>
          </div>

          <fieldset className="chips-field">
            <legend className="field__label">Tipo</legend>
            <div className="chips" role="radiogroup" aria-label="Tipo do provento">
              {INCOME_KINDS.map((k) => (
                <button // NOSONAR: chip de escolha como os outros seletores de chip do app (Tab e clique em cada um); <input type="radio"> trocaria o teclado (setas mudam o tipo)
                  key={k}
                  type="button"
                  role="radio"
                  aria-checked={k === kind}
                  className={cx("chip", k === kind && "is-active")}
                  onClick={() => setKind(k)}
                >
                  {KIND_LABEL[k]}
                </button>
              ))}
            </div>
          </fieldset>

          <div className="field">
            <label htmlFor={ids.pay} className="field__label">
              Data de pagamento
            </label>
            <input id={ids.pay} className="text-input" type="date" value={payDate} onChange={(event) => setPayDate(event.target.value)} />
          </div>

          <div className="field">
            <label htmlFor={ids.unit} className="field__label">
              {byValue ? "Valor recebido (R$)" : "Valor por cota (R$)"}
            </label>
            <input
              id={ids.unit}
              className="text-input"
              type="text"
              inputMode="decimal"
              autoComplete="off"
              placeholder="0,00"
              value={perUnit}
              onChange={(event) => setPerUnit(event.target.value)}
            />
          </div>

          {!byValue && (
            <QuantityField
              id={ids.quantity}
              value={quantity}
              onChange={setQuantity}
              bookQuantity={bookQuantity}
              entitlement={entitlement.data}
              noSharesOnRecordDate={noSharesOnRecordDate}
            />
          )}

          <div className="field">
            <label htmlFor={ids.ex} className="field__label">
              Data ex (opcional)
            </label>
            <input id={ids.ex} className="text-input" type="date" value={exDate} onChange={(event) => setExDate(event.target.value)} />
            {exDate !== "" && exDate > payDate && <span className="form-error">A data ex não pode ser depois do pagamento.</span>}
          </div>

          <WithholdingField
            id={ids.withholding}
            text={withholdingText}
            onChange={setWithholdingText}
            defaultPreview={defaultPreview}
            usListed={usListed}
            aboveGross={withholdingAboveGross}
          />

          <PreviewLine preview={preview} />
          {kind === "jcp" && <p className="field__hint">JCP tem IR retido na fonte (17,5% desde 2026; 15% até 2025); o líquido já desconta.</p>}

          {create.isError && (
            <p className="form-error" role="alert">
              {errorMessage(create.error)}
            </p>
          )}

          <MagneticButton type="submit" disabled={!request || withholdingAboveGross} loading={create.isPending}>
            Lançar provento
          </MagneticButton>
        </form>
      )}
    </Sheet>
  );
}

type QuantityFieldProps = {
  id: string;
  value: string;
  onChange: (value: string) => void;
  /** As cotas do livro na data com, no exemplo do campo; `null` enquanto o servidor não diz. */
  bookQuantity: number | null;
  /** O que o servidor disse do livro na data com; `undefined` enquanto não diz. */
  entitlement: Entitlement | undefined;
  noSharesOnRecordDate: boolean;
};

/** A quantidade: em branco, vale o que o livro tinha na data com. */
function QuantityField({ id, value, onChange, bookQuantity, entitlement, noSharesOnRecordDate }: Readonly<QuantityFieldProps>) {
  const money = useMoney();
  return (
    <div className="field">
      <label htmlFor={id} className="field__label">
        Quantidade
      </label>
      <input
        id={id}
        className="text-input"
        type="text"
        inputMode="decimal"
        autoComplete="off"
        placeholder={bookQuantity !== null ? money.quantity(bookQuantity, !Number.isInteger(bookQuantity)) : "0"}
        value={value}
        onChange={(event) => onChange(event.target.value)}
      />
      <span className="field__hint">
        {entitlement
          ? `Em branco vale o que o livro tinha na data com (${formatIsoDate(entitlement.record_date)}).`
          : "Em branco vale o que o livro tinha na data com."}
      </span>
      {noSharesOnRecordDate && (
        <span className="form-error" role="alert">
          Você não tinha cotas na data com. Informe a quantidade se mesmo assim recebeu.
        </span>
      )}
    </div>
  );
}

type WithholdingFieldProps = {
  id: string;
  /** `null` = a pessoa não mexeu: o campo mostra o imposto padrão da prévia. */
  text: string | null;
  onChange: (text: string) => void;
  /** A prévia com o imposto padrão; `null` enquanto não há prévia. */
  defaultPreview: Amounts | null;
  usListed: boolean;
  aboveGross: boolean;
};

/** O imposto retido: vem com o padrão do tipo e do mercado, e a pessoa troca pelo do extrato. */
function WithholdingField({ id, text, onChange, defaultPreview, usListed, aboveGross }: Readonly<WithholdingFieldProps>) {
  const money = useMoney();
  return (
    <div className="field">
      <div className="field__label-row">
        <label htmlFor={id} className="field__label">
          Imposto retido (R$)
        </label>
        {usListed && (
          <InfoTip label="Por que 30% de IR retido">
            Dividendo de ação ou REIT dos EUA chega com 30% retidos lá. Se o seu extrato mostrar outro valor, digite-o aqui.
          </InfoTip>
        )}
      </div>
      <input
        id={id}
        className="text-input"
        type="text"
        inputMode="decimal"
        autoComplete="off"
        placeholder={defaultPreview ? money.brl(defaultPreview.withholding) : "0,00"}
        value={text ?? (defaultPreview ? defaultPreview.withholding.toFixed(2).replace(".", ",") : "")}
        onChange={(event) => onChange(event.target.value)}
      />
      <span className="field__hint">Em branco vale o padrão do tipo; digite 0 se não houve retenção.</span>
      {aboveGross && <span className="form-error">O imposto retido não pode passar do bruto.</span>}
    </div>
  );
}

/** A prévia de bruto, imposto retido e líquido; sem valor, o que falta preencher. */
function PreviewLine({ preview }: Readonly<{ preview: Amounts | null }>) {
  const money = useMoney();
  return (
    <div className="income-preview" aria-live="polite">
      {preview ? (
        <>
          <span>
            Bruto <strong>{money.brl(preview.gross)}</strong>
          </span>
          <span>
            IR retido <strong>{money.brl(preview.withholding)}</strong>
          </span>
          <span>
            Líquido <strong className="gold">{money.brl(preview.net)}</strong>
          </span>
        </>
      ) : (
        <span className="field__hint">Preencha o valor para ver o líquido.</span>
      )}
    </div>
  );
}
