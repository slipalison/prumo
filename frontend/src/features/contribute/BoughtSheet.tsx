import { useId, useState, type FormEvent } from "react";
import type { PlanItem } from "../../api/types";
import { AssetIcon } from "../../components/AssetIcon";
import { MagneticButton } from "../../components/MagneticButton";
import { MoneyInput } from "../../components/MoneyInput";
import { QuantityInput } from "../../components/QuantityInput";
import { Sheet } from "../../components/Sheet";
import { nounOf, unitOf } from "../../lib/assetClasses";
import { useMoney } from "../../lib/privacy";
import { assetName } from "../../lib/rating";
import { markValue, suggested, type Mark } from "./marks";

type BoughtSheetProps = {
  open: boolean;
  /** O ativo da pergunta; vazio enquanto nenhuma foi feita (a folha existe fechada para guardar o foco de quem a abre). */
  item: PlanItem | null;
  /** A compra já marcada, quando a pessoa edita; sem ela, vale a sugestão do plano. */
  mark?: Mark;
  onClose: () => void;
  onConfirm: (mark: Mark) => void;
};

/**
 * "Comprei": a pergunta curta de um ativo do plano — quanto e a que preço. Vem
 * preenchida com a sugestão do plano (ou com a compra já marcada, ao editar), e
 * os dois campos são editáveis: a corretora pode ter executado outra quantidade
 * ou outro preço. Uma folha pequena, não um balão: o campo numérico no celular
 * sobe o teclado, e a folha prende o foco e fecha com Esc.
 */
export function BoughtSheet({ open, item, mark, onClose, onConfirm }: BoughtSheetProps) {
  return (
    <Sheet
      open={open}
      onClose={onClose}
      title={item ? `Comprei ${assetName(item)}` : "Comprei"}
      description="Confira a quantidade e o preço pagos. Vêm do plano; ajuste se a corretora executou diferente."
    >
      {open && item && <BoughtForm item={item} mark={mark} onConfirm={onConfirm} />}
    </Sheet>
  );
}

function BoughtForm({ item, mark, onConfirm }: { item: PlanItem; mark?: Mark; onConfirm: (mark: Mark) => void }) {
  const baseId = useId();
  const money = useMoney();
  const initial = mark ?? suggested(item);
  const [quantity, setQuantity] = useState(initial.quantity);
  const [price, setPrice] = useState(initial.unit_price);

  const unit = unitOf(item.asset_class, item.listed_on_b3, item.treasury);
  const noun = nounOf(item.asset_class, item.listed_on_b3, item.treasury);
  const byValue = unit === "value";
  // Com os valores ocultos o campo só mostra o número enquanto tem o foco: não
  // abre já com o foco, para a folha não revelar a quantidade por conta própria.
  const autofocus = !money.hidden;
  const valid = quantity > 0 && price > 0;
  const total = markValue({ quantity, unit_price: price });

  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (valid) onConfirm({ quantity, unit_price: price });
  };

  return (
    <form className="bought-form" onSubmit={submit}>
      <div className="bought-form__id">
        <AssetIcon symbol={item.symbol} name={item.display_name} assetClass={item.asset_class} treasury={item.treasury} />
        <span className="bought-form__text">
          <strong>{assetName(item)}</strong>
          <span className="field__hint">{item.display_name}</span>
        </span>
      </div>

      {byValue ? (
        <MoneyInput id={`${baseId}-quantity`} label="Valor aplicado" value={quantity} onChange={setQuantity} focusOnMount={autofocus} />
      ) : (
        <>
          <QuantityInput
            id={`${baseId}-quantity`}
            label={unit === "fraction" ? "Quantidade" : noun[1].replace(/^./, (c) => c.toUpperCase())}
            value={quantity}
            onChange={setQuantity}
            fractional={unit === "fraction"}
            focusOnMount={autofocus}
          />
          <MoneyInput id={`${baseId}-price`} label="Preço pago" value={price} onChange={setPrice} />
        </>
      )}

      <div className="bought-form__total">
        <span>Total</span>
        <strong>{money.brl(total)}</strong>
      </div>

      <MagneticButton type="submit" disabled={!valid} className="wide">
        {mark ? "Salvar" : "Marcar como comprado"}
      </MagneticButton>
    </form>
  );
}
