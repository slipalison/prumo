import { describe, expect, it } from "vitest";
import { ANALYSIS_RULE_IDS } from "../api/analysisTypes";
import { defaultAnalysisRules, result } from "../test/analysisHandlers";
import {
  attentionText,
  COUNT_MAX,
  formatSignedPercent,
  limitCell,
  limitLabel,
  noDataText,
  pointText,
  restText,
  NO_DATA_TEXT,
  ruleAction,
  ruleExplanation,
  ruleName,
  ruleSentence,
  RULE_TEXT,
  scorePercent,
  scoreText,
  STATUS_TEXT,
} from "./analysisText";
import { createMoney } from "./money";

const visible = createMoney(false);
const hidden = createMoney(true);

describe("texto da Análise", () => {
  it("toda regra tem frase, motivo de falta de dado e limite de contagem coerente", () => {
    expect(Object.keys(RULE_TEXT).sort()).toEqual([...ANALYSIS_RULE_IDS].sort());
    expect(Object.keys(NO_DATA_TEXT).sort()).toEqual([...ANALYSIS_RULE_IDS].sort());
    for (const rule of defaultAnalysisRules()) {
      expect(RULE_TEXT[rule.rule].lead.length).toBeGreaterThan(0);
      expect(RULE_TEXT[rule.rule].why.length).toBeGreaterThan(0);
      // Só regra de contagem com limite editável tem teto de contagem.
      if (COUNT_MAX[rule.rule] !== undefined) {
        expect(rule.unit).toBe("count");
        expect(rule.default_threshold).not.toBeNull();
      }
    }
    expect(STATUS_TEXT).toEqual({ off: "Desligada", no_data: "Sem dado", ok: "Ok", warning: "Ponto a ver" });
  });

  it("monta a frase com o limite da pessoa e o nome para leitor de tela", () => {
    const rules = defaultAnalysisRules();
    const find = (id: string) => rules.find((r) => r.rule === id);
    const abroad = find("min_abroad");
    const cdi = find("beat_cdi_12m");
    const empty = find("empty_class");
    if (!abroad || !cdi || !empty) throw new Error("regra de fábrica ausente");
    expect(ruleSentence(abroad)).toBe("Pelo menos 10% da carteira no exterior");
    expect(ruleSentence({ ...abroad, threshold: 12.5 })).toBe("Pelo menos 12,50% da carteira no exterior");
    expect(ruleSentence(cdi)).toBe("A carteira rende ao menos o CDI em 12 meses");
    expect(ruleSentence(empty)).toBe("Toda classe com meta tem ativo");
    expect(ruleName("min_abroad")).toBe("Pelo menos … da carteira no exterior");
    expect(ruleName("empty_class")).toBe("Toda classe com meta tem ativo");
  });

  it("o segundo número é o limite, o mínimo ou o índice", () => {
    const rules = defaultAnalysisRules();
    const label = (id: string) => limitLabel(rules.find((r) => r.rule === id) as (typeof rules)[number]);
    expect(label("max_asset")).toBe("Limite");
    expect(label("min_abroad")).toBe("Mínimo");
    expect(label("beat_cdi_12m")).toBe("CDI");
    expect(label("beat_ibov_5y")).toBe("Ibovespa");
    expect(label("beat_ipca_12m")).toBe("IPCA");
    expect(label("empty_class")).toBeNull();
    expect(label("brick_reit")).toBeNull();
  });

  it("na célula da tabela, o limite leva o sentido escrito", () => {
    expect(limitCell(result("max_asset", "warning", { current: 70 }))).toBe("máx. 15%");
    expect(limitCell(result("min_abroad", "warning", { current: 0 }))).toBe("mín. 10%");
    expect(limitCell(result("min_assets", "ok", { current: 3 }))).toBe("mín. 3");
    expect(limitCell(result("beat_cdi_12m", "warning", { current: 9.4, limit: 13.2 }))).toBe("CDI 13,20%");
    // Sem limite (regra sem dado, ou sem limite de fábrica): célula vazia.
    expect(limitCell(result("min_abroad", "no_data"))).toBeNull();
    expect(limitCell(result("brick_reit", "ok", { current: 1 }))).toBeNull();
  });

  it("o recolher de cada dimensão conta o que está ok, sem dado e desligado", () => {
    expect(restText([result("max_asset", "ok"), result("top_five", "ok"), result("min_sectors", "no_data")])).toBe(
      "2 regras ok · 1 sem dado",
    );
    expect(restText([result("max_asset", "ok"), result("min_classes", "off"), result("min_sectors", "off")])).toBe(
      "1 regra ok · 2 desligadas",
    );
    expect(restText([result("min_sectors", "off")])).toBe("1 desligada");
  });

  it("diz a pontuação com o número exato e o que ficou fora da conta", () => {
    expect(scoreText({ ok: 5, measured: 8, no_data: 2, percent: 62.5 })).toBe("5 de 8 regras ok");
    expect(scoreText({ ok: 1, measured: 1, no_data: 0, percent: 100 })).toBe("1 de 1 regra ok");
    expect(scorePercent({ ok: 5, measured: 8, no_data: 2, percent: 62.5 })).toBe("62,5%");
    expect(scorePercent({ ok: 2, measured: 2, no_data: 0, percent: 100 })).toBe("100%");
    expect(scorePercent({ ok: 0, measured: 0, no_data: 3, percent: null })).toBeNull();
    expect(noDataText({ ok: 5, measured: 8, no_data: 2, percent: 62.5 })).toBe("2 regras sem dado, fora da conta");
    expect(noDataText({ ok: 5, measured: 8, no_data: 1, percent: 62.5 })).toBe("1 regra sem dado, fora da conta");
    expect(noDataText({ ok: 5, measured: 8, no_data: 0, percent: 62.5 })).toBeNull();
    expect(attentionText(1)).toBe("1 ponto a ver");
    expect(attentionText(3)).toBe("3 pontos a ver");
  });

  it("o sinal diz o sentido sem depender da cor", () => {
    expect(formatSignedPercent(9.4)).toBe("+9,40%");
    expect(formatSignedPercent(-12.8)).toBe("−12,80%");
    expect(formatSignedPercent(0.001)).toBe("0,00%");
  });
});

describe("a conta por trás de cada regra", () => {
  it("diversificação", () => {
    expect(ruleExplanation(result("min_portfolio_assets", "warning", { current: 4 }), visible)).toBe(
      "Você tem 4 ativos com valor.",
    );
    expect(ruleExplanation(result("min_portfolio_assets", "warning", { current: 1 }), visible)).toBe(
      "Você tem 1 ativo com valor.",
    );
    expect(
      ruleExplanation(
        result("min_classes", "ok", { current: 2, detail: { kind: "classes", asset_classes: ["br_stocks", "crypto"] } }),
        visible,
      ),
    ).toBe("Dinheiro em 2 classes: Ações Nacionais, Criptomoedas.");
    expect(
      ruleExplanation(
        result("min_sectors", "warning", { current: 1, detail: { kind: "sectors", sectors: ["Financeiro"] } }),
        visible,
      ),
    ).toBe("Suas ações estão em 1 setor: Financeiro.");
    expect(ruleExplanation(result("min_abroad", "warning", { current: 3.5 }), visible)).toBe(
      "3,50% da carteira está no exterior.",
    );
    expect(ruleExplanation(result("min_low_risk", "ok", { current: 20 }), visible)).toBe(
      "20% da carteira está em renda fixa.",
    );
    // As sete originais têm cartão próprio.
    expect(ruleExplanation(result("max_asset", "warning", { current: 40 }), visible)).toBeNull();
  });

  it("dividendos: o valor em reais some com o olho fechado", () => {
    const yieldRule = result("min_yield", "warning", {
      current: 4.2,
      detail: { kind: "yield", income_12m: 1200, base_value: 28_000 },
    });
    expect(ruleExplanation(yieldRule, visible)).toMatch(
      /Você recebeu R\$\s1,2\smil em 12 meses sobre R\$\s28\smil em ações e FIIs\./,
    );
    expect(ruleExplanation(yieldRule, hidden)).toBe("Você recebeu R$ •••• em 12 meses sobre R$ •••• em ações e FIIs.");
    expect(
      ruleExplanation(
        result("min_paying_share", "warning", { current: 50, detail: { kind: "paying", paying: 1, total: 2, silent: ["XPTO3"] } }),
        visible,
      ),
    ).toBe("1 de 2 ativos pagaram provento em 12 meses. Sem provento: XPTO3.");
    expect(
      ruleExplanation(
        result("min_paying_share", "ok", { current: 100, detail: { kind: "paying", paying: 1, total: 1, silent: [] } }),
        visible,
      ),
    ).toBe("1 de 1 ativo pagou provento em 12 meses.");
    expect(
      ruleExplanation(
        result("income_growth", "warning", { detail: { kind: "income_growth", current_12m: 900, previous_12m: 1000 } }),
        hidden,
      ),
    ).toBe("Renda de 12 meses: R$ ••••; dos 12 meses anteriores: R$ ••••.");
    expect(
      ruleExplanation(result("brick_reit", "warning", { current: 0, detail: { kind: "brick", reits: 1, brick: 0 } }), visible),
    ).toBe("0 de 1 FII é de tijolo.");
  });

  it("rentabilidade: datas, sinais e o índice pelo nome", () => {
    const benchmark = result("beat_ipca_12m", "ok", {
      current: 9.4,
      limit: 4.3,
      detail: {
        kind: "benchmark",
        benchmark: "ipca",
        horizon: "12m",
        from: "2025-10-31",
        to: "2026-10-31",
        portfolio_percent: 9.4,
        benchmark_percent: 4.3,
      },
    });
    expect(ruleExplanation(benchmark, visible)).toBe(
      "De 31/10/2025 a 31/10/2026 a carteira rendeu +9,40%; o IPCA, +4,30%.",
    );
    expect(
      ruleExplanation(
        result("min_positive_years", "warning", { current: 66.67, detail: { kind: "years", positive: 2, total: 3 } }),
        visible,
      ),
    ).toBe("2 de 3 anos civis fechados com retorno positivo.");
    expect(ruleExplanation(result("max_volatility", "ok", { current: 8.4 }), visible)).toBe(
      "Os retornos mensais oscilaram o equivalente a 8,40% ao ano.",
    );
    expect(
      ruleExplanation(
        result("max_loss", "warning", {
          current: 50,
          detail: {
            kind: "losses",
            assets: [
              { asset_id: "a", label: "AAAA3", asset_class: "br_stocks", loss_percent: 50 },
              { asset_id: "b", label: "BBBB3", asset_class: "br_stocks", loss_percent: 35.5 },
            ],
          },
        }),
        visible,
      ),
    ).toBe("AAAA3 está 50% abaixo do que você pagou; BBBB3 está 35,50% abaixo do que você pagou.");
    expect(ruleExplanation(result("max_loss", "ok", { current: 12 }), visible)).toBe(
      "A maior perda desde a compra é de 12%.",
    );
    // Sem número, sem frase.
    expect(ruleExplanation(result("beat_cdi_12m", "no_data"), visible)).toBeNull();
  });
});

describe("o que fazer num ponto a ver", () => {
  it("aportar fora, sem vender: o valor some com o olho fechado", () => {
    const abroad = result("min_abroad", "warning", { current: 0, needed_contribution: 14_500 });
    expect(ruleAction(abroad, visible)?.text).toMatch(/aporte cerca de R\$\s14,5\smil em ativos de fora/);
    expect(ruleAction(abroad, hidden)?.text).toBe(
      "Para chegar ao seu mínimo no exterior sem vender nada, aporte cerca de R$ •••• em ativos de fora.",
    );
    expect(ruleAction(abroad, visible)?.link).toEqual({ href: "#/aportar", label: "Calcular o próximo aporte" });
    // Limite de 100% não tem aporte que chegue lá: sem número, a frase continua de pé.
    expect(ruleAction(result("min_low_risk", "warning", { current: 5 }), visible)?.text).toBe(
      "Para chegar ao seu mínimo em renda fixa sem vender nada, aporte em renda fixa.",
    );
  });

  it("conta quantos faltam e nunca cita ativo de fora", () => {
    expect(ruleAction(result("min_portfolio_assets", "warning", { current: 4, limit: 10 }), visible)?.text).toMatch(
      /^Adicione mais 6 ativos:/,
    );
    expect(ruleAction(result("min_portfolio_assets", "warning", { current: 9, limit: 10 }), visible)?.text).toMatch(
      /^Adicione mais 1 ativo:/,
    );
    const classes = ruleAction(result("min_classes", "warning", { current: 2, limit: 3 }), visible);
    expect(classes?.text).toMatch(/^Abra posição em mais 1 classe:/);
    expect(classes?.link?.href).toBe("#/ajustes/metas");
    expect(ruleAction(result("min_sectors", "warning", { current: 2, limit: 4 }), visible)?.text).toBe(
      "Para chegar a 4 setores, leve o próximo aporte a ações de setores que você ainda não tem.",
    );
  });

  it("rentabilidade e perdas mandam olhar, não mandam trocar nem vender", () => {
    const cdi = ruleAction(result("beat_cdi_12m", "warning", { current: 9, limit: 13 }), visible);
    expect(cdi?.text).toMatch(/não manda trocar nada/);
    expect(cdi?.link).toEqual({ href: "#/desempenho", label: "Abrir o Desempenho" });
    const loss = ruleAction(result("max_loss", "warning", { current: 50, limit: 30 }), visible);
    expect(loss?.text).toMatch(/O Prumo não manda vender/);
    expect(loss?.link?.href).toBe("#/carteira");
    expect(ruleAction(result("min_yield", "warning", { current: 2, limit: 5 }), visible)?.link?.href).toBe("#/renda");
    expect(ruleAction(result("brick_reit", "warning", { current: 0 }), visible)?.link?.href).toBe("#/aportar");
    // As originais da Saúde têm o "o que fazer" nos próprios cartões.
    expect(ruleAction(result("max_asset", "warning", { current: 40 }), visible)).toBeNull();
  });
});

describe("qualidade e pontos de atenção", () => {
  it("diz quantos ativos entraram na conta e quem ficou abaixo do critério", () => {
    const roe = result("quality_roe", "warning", {
      current: 60,
      detail: { kind: "quality", covered: 2, total: 3, failing: ["AAAA3", "BBBB3"] },
    });
    expect(ruleExplanation(roe, visible)).toBe("Entraram na conta 2 de 3 ativos. Abaixo do critério: AAAA3, BBBB3.");
    const ok = result("quality_fii_vacancy", "ok", {
      current: 100,
      detail: { kind: "quality", covered: 1, total: 1, failing: [] },
    });
    expect(ruleExplanation(ok, visible)).toBe("Entraram na conta 1 de 1 ativo.");
    expect(ruleExplanation(result("quality_roe", "no_data"), visible)).toBeNull();
  });

  it("recuperação judicial nomeia a empresa; sem nenhuma, diz quantas têm cadastro", () => {
    const bad = result("quality_recovery", "warning", {
      current: 1,
      detail: { kind: "quality", covered: 3, total: 3, failing: ["RJ3"] },
    });
    expect(ruleExplanation(bad, visible)).toBe("Em recuperação judicial: RJ3.");
    const fine = result("quality_recovery", "ok", {
      current: 0,
      detail: { kind: "quality", covered: 3, total: 4, failing: [] },
    });
    expect(ruleExplanation(fine, visible)).toBe("Nenhuma empresa está em recuperação judicial (3 com cadastro na CVM).");
  });

  it("o que fazer em Qualidade manda olhar os fundamentos, não vender", () => {
    const roe = ruleAction(result("quality_roe", "warning", { current: 10, limit: 70 }), visible);
    expect(roe?.text).toMatch(/não manda vender/);
    expect(roe?.text).toMatch(/Fundamentos/);
    expect(roe?.link?.href).toBe("#/carteira");
    expect(ruleAction(result("quality_recovery", "warning", { current: 1 }), visible)?.text).toMatch(/O Prumo não manda vender/);
  });

  it("cada ponto de atenção é uma frase curta e factual, sem recomendação", () => {
    expect(pointText({ kind: "income_drop", percent: 22.5 })).toBe(
      "Provento de 12 meses caiu 22,50% em relação aos 12 meses anteriores",
    );
    expect(pointText({ kind: "low_revenue_growth", percent: 2.34 })).toBe("Receita cresceu 2,34% ao ano em 5 anos, abaixo de 5%");
    expect(pointText({ kind: "low_earnings_growth", percent: -3 })).toBe("Lucro encolheu 3% ao ano em 5 anos, abaixo de 5%");
    expect(pointText({ kind: "low_roe", percent: 4 })).toBe("ROE de 4%, abaixo de 10%");
    expect(pointText({ kind: "low_net_margin", percent: 8.33 })).toBe("Margem líquida de 8,33%, abaixo de 10%");
    expect(pointText({ kind: "debt_above_equity", times: 2.9 })).toBe("Dívida líquida de 2,9 vezes o patrimônio líquido");
    expect(pointText({ kind: "debt_above_equity", times: 3 })).toBe("Dívida líquida de 3 vezes o patrimônio líquido");
    expect(pointText({ kind: "debt_above_equity", times: 10.5 })).toBe("Dívida líquida de 10,5 vezes o patrimônio líquido");
    expect(pointText({ kind: "judicial_recovery" })).toBe("Em recuperação judicial");
    for (const point of [
      { kind: "judicial_recovery" as const },
      { kind: "low_roe" as const, percent: 1 },
      { kind: "income_drop" as const, percent: 30 },
    ]) {
      expect(pointText(point)).not.toMatch(/compre|venda|oportunidade/i);
    }
  });
});
