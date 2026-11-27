import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { IncomeMonth } from "../../api/incomeTypes";
import { createMoney, MASK_BRL } from "../../lib/money";
import { PrivacyContext } from "../../lib/privacy";
import { MonthlyBars } from "./MonthlyBars";

/** Três meses fictícios: um forte, um fraco e um com parte a receber. */
const MONTHS: IncomeMonth[] = [
  { month: "2025-12", received: 1580.4, upcoming: 0 },
  { month: "2026-01", received: 120, upcoming: 0 },
  { month: "2026-02", received: 300, upcoming: 210.55 },
];

function renderBars(hidden = false) {
  return render(
    <PrivacyContext.Provider value={{ money: createMoney(hidden), toggle: () => undefined }}>
      <MonthlyBars months={MONTHS} />
    </PrivacyContext.Provider>,
  );
}

const tooltip = () => document.querySelector(".income-bars__tooltip");

describe("Mês a mês: o valor no ponteiro", () => {
  it("mostra o recebido do mês sob o ponteiro, na coluna inteira, e some ao sair", () => {
    renderBars();
    expect(tooltip()).toBeNull();
    fireEvent.pointerEnter(screen.getByTestId("income-bars-hit-2025-12"));
    expect(tooltip()).toHaveTextContent("Dezembro de 2025");
    expect(tooltip()).toHaveTextContent(/Recebido\s*R\$\s1\.580,40/);
    expect(tooltip()).not.toHaveTextContent("A receber");
    // Mês fraco: a barra é baixa, mas a coluna inteira responde.
    fireEvent.pointerEnter(screen.getByTestId("income-bars-hit-2026-01"));
    expect(tooltip()).toHaveTextContent(/Recebido\s*R\$\s120,00/);
    fireEvent.pointerLeave(document.querySelector(".income-bars__svg") as Element);
    expect(tooltip()).toBeNull();
  });

  it("traz o a receber quando há", () => {
    renderBars();
    fireEvent.pointerEnter(screen.getByTestId("income-bars-hit-2026-02"));
    expect(tooltip()).toHaveTextContent(/Recebido\s*R\$\s300,00/);
    expect(tooltip()).toHaveTextContent(/A receber\s*R\$\s210,55/);
  });

  it("no toque, o mês tocado abre a caixa", () => {
    renderBars();
    fireEvent.pointerDown(screen.getByTestId("income-bars-hit-2026-01"));
    expect(tooltip()).toHaveTextContent(/R\$\s120,00/);
  });

  it("com os valores ocultos, a caixa mostra a máscara", () => {
    renderBars(true);
    fireEvent.pointerEnter(screen.getByTestId("income-bars-hit-2025-12"));
    expect(tooltip()).toHaveTextContent(MASK_BRL);
    expect(tooltip()).not.toHaveTextContent(/R\$\s*\d/);
  });

  it("a caixa é só para quem vê: o leitor de tela tem a tabela", () => {
    renderBars();
    fireEvent.pointerEnter(screen.getByTestId("income-bars-hit-2025-12"));
    expect(tooltip()).toHaveAttribute("aria-hidden", "true");
    expect(screen.getByRole("table", { name: /Renda líquida por mês/ })).toBeInTheDocument();
  });
});
