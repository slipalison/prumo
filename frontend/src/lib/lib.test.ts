import confetti from "canvas-confetti";
import { afterEach, describe, expect, it, vi } from "vitest";
import { defaultAssets, defaultChecklists, samplePlan, toAsset } from "../test/fixtures";
import {
  acceptsB3Listing,
  CLASS_META,
  classColor,
  guessClassFromSymbol,
  isTreasuryKey,
  isValueBased,
  looksLikeB3Ticker,
  nounOf,
  unitOf,
} from "./assetClasses";
import { celebrate, originFrom, prefersReducedMotion } from "./celebrate";
import { cx } from "./cx";
import {
  formatBRL,
  formatCompactBRL,
  formatDateTime,
  formatDay,
  formatPercent,
  formatQuantity,
  formatRelative,
  parseDecimal,
  plural,
} from "./format";
import { balanceTargets, isComplete, PRESETS, sameTargets, sumTargets, targetsFromClasses } from "./goals";
import { centsFromInput, formatMoneyInput, reaisFromInput } from "./moneyMask";
import { createMoney, MASK_BRL, MASK_QUANTITY } from "./money";
import { looksMissingB3Flag, noticeText, unitLine } from "./planText";
import { HIDE_VALUES_KEY, readHideValues, saveHideValues } from "./privacy";
import { assetName } from "./rating";
import { hashSegments, navigate, parseRoute, parseSection } from "./route";
import { answeredCount, answersDiff, checklistScore } from "./score";
import { applyTheme, readThemePreference, saveThemePreference, THEME_KEY } from "./theme";

/** Falha o teste em vez de usar `!`: índice fora da lista é defeito do teste. */
function must<T>(value: T | undefined): T {
  if (value === undefined) throw new Error("valor ausente na fixture");
  return value;
}

const nbsp = (text: string) => text.replace(/\u00a0/g, " ");

describe("format", () => {
  it("formata reais em pt-BR", () => {
    expect(nbsp(formatBRL(1234.5))).toBe("R$ 1.234,50");
    expect(nbsp(formatBRL(Number.NaN))).toBe("R$ 0,00");
  });

  it("compacta valores grandes e mantém os pequenos", () => {
    expect(nbsp(formatCompactBRL(25226.45))).toBe("R$ 25,2 mil");
    expect(nbsp(formatCompactBRL(950.4))).toBe("R$ 950,40");
  });

  it("formata percentuais de 0 a 100", () => {
    expect(formatPercent(5.36)).toBe("5,4%");
    expect(formatPercent(5, 0)).toBe("5%");
    expect(formatPercent(Number.POSITIVE_INFINITY)).toBe("0,0%");
  });

  it("formata quantidade inteira e fracionada", () => {
    expect(formatQuantity(1500, false)).toBe("1.500");
    expect(formatQuantity(0.0045, true)).toBe("0,0045");
    expect(formatQuantity(2.5, true)).toBe("2,5");
  });

  it("escolhe singular e plural", () => {
    expect(plural(1, ["cota", "cotas"])).toBe("cota");
    expect(plural(12, ["cota", "cotas"])).toBe("cotas");
  });

  it("diz há quanto tempo", () => {
    const now = new Date("2026-11-25T12:00:00Z");
    expect(formatRelative(null, now)).toBe("nunca");
    expect(formatRelative("lixo", now)).toBe("nunca");
    expect(formatRelative("2026-11-25T11:59:30Z", now)).toBe("agora há pouco");
    expect(formatRelative("2026-11-25T11:48:00Z", now)).toBe("há 12 min");
    expect(formatRelative("2026-11-25T09:00:00Z", now)).toBe("há 3 h");
    expect(formatRelative("2026-11-20T09:00:00Z", now)).toBe(formatDateTime("2026-11-20T09:00:00Z"));
    expect(formatDateTime("lixo")).toBe("");
  });

  it("lê decimal digitado em pt-BR ou com ponto", () => {
    expect(parseDecimal("1.234,5")).toBe(1234.5);
    expect(parseDecimal("0,015")).toBe(0.015);
    expect(parseDecimal("12.5")).toBe(12.5);
    expect(parseDecimal("  ")).toBeNull();
    expect(parseDecimal("abc")).toBeNull();
  });
});

describe("moneyMask", () => {
  it("entra pela direita, como centavo", () => {
    expect(centsFromInput("600000")).toBe(600000);
    expect(reaisFromInput("R$ 6.000,00")).toBe(6000);
    expect(nbsp(formatMoneyInput(6000))).toBe("R$ 6.000,00");
  });

  it("ignora o que não é dígito e corta zeros e excesso", () => {
    expect(centsFromInput("abc")).toBe(0);
    expect(centsFromInput("000123")).toBe(123);
    expect(centsFromInput("99999999999999999")).toBe(9999999999999);
  });
});

describe("goals", () => {
  const preset = must(PRESETS[2]);

  it("soma com duas casas", () => {
    expect(sumTargets({ ...must(PRESETS[0]).targets })).toBe(100);
    expect(isComplete({ br_stocks: 33.33, intl_stocks: 33.33, br_reits: 33.34, us_reits: 0, crypto: 0, fixed_income: 0, intl_fixed_income: 0 })).toBe(true);
  });

  it("equilibra mantendo a proporção e fechando 100", () => {
    const balanced = balanceTargets({ br_stocks: 30, intl_stocks: 30, br_reits: 15, us_reits: 0, crypto: 10, fixed_income: 10, intl_fixed_income: 0 });
    expect(sumTargets(balanced)).toBe(100);
    expect(balanced.us_reits).toBe(0);
    expect(balanced.br_stocks).toBe(balanced.intl_stocks);
  });

  it("sem meta nenhuma, equilibrar não inventa proporção", () => {
    const zero = { br_stocks: 0, intl_stocks: 0, br_reits: 0, us_reits: 0, crypto: 0, fixed_income: 0, intl_fixed_income: 0 };
    expect(balanceTargets(zero)).toEqual(zero);
  });

  it("lê as metas das classes e compara", () => {
    const classes = Object.entries(preset.targets).map(([asset_class, target_percent]) => ({
      asset_class: asset_class as keyof typeof preset.targets,
      target_percent,
      current_value: 0,
      current_percent: 0,
      asset_count: 0,
      eligible: false,
    }));
    expect(sameTargets(targetsFromClasses(classes), preset.targets)).toBe(true);
    expect(sameTargets(must(PRESETS[0]).targets, must(PRESETS[1]).targets)).toBe(false);
  });
});

describe("score", () => {
  const questions = [
    { id: "a", position: 1, prompt: "A?" },
    { id: "b", position: 2, prompt: "B?" },
    { id: "c", position: 3, prompt: "C?" },
  ];

  it("sim soma, não tira, nunca negativa, ignora pergunta que sumiu", () => {
    expect(checklistScore({ a: true, b: true, c: false }, questions)).toBe(1);
    expect(checklistScore({ a: false, b: false }, questions)).toBe(0);
    expect(checklistScore({ zzz: true, a: null }, questions)).toBe(0);
    expect(answeredCount({ a: true, b: null }, questions)).toBe(1);
  });

  it("calcula o diff para o PATCH", () => {
    expect(answersDiff({ a: true }, { a: true })).toBeNull();
    expect(answersDiff({ a: true, b: false }, { a: null, b: true, c: false })).toEqual({ a: null, b: true, c: false });
  });
});

describe("theme", () => {
  afterEach(() => vi.restoreAllMocks());

  it("lê a preferência salva, com escuro de padrão", () => {
    expect(readThemePreference()).toBe("dark");
    window.localStorage.setItem(THEME_KEY, "system");
    expect(readThemePreference()).toBe("system");
    window.localStorage.setItem(THEME_KEY, "invalido");
    expect(readThemePreference()).toBe("dark");
  });

  it("sistema remove o atributo; os outros escrevem", () => {
    applyTheme("light");
    expect(document.documentElement.dataset.theme).toBe("light");
    applyTheme("system");
    expect(document.documentElement.hasAttribute("data-theme")).toBe(false);
    saveThemePreference("dark");
    expect(window.localStorage.getItem(THEME_KEY)).toBe("dark");
    expect(document.documentElement.dataset.theme).toBe("dark");
  });

  it("sobrevive a armazenamento bloqueado", () => {
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new Error("bloqueado");
    });
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("bloqueado");
    });
    expect(readThemePreference()).toBe("dark");
    saveThemePreference("light");
    expect(document.documentElement.dataset.theme).toBe("light");
  });
});

describe("route", () => {
  const ids = ["aportar", "carteira", "ajustes"] as const;

  it("entende o hash e cai na abertura quando não conhece", () => {
    expect(parseRoute("#/carteira", ids)).toBe("carteira");
    expect(parseRoute("#ajustes", ids)).toBe("ajustes");
    expect(parseRoute("#/ajustes/metas?x=1", ids)).toBe("ajustes");
    expect(parseRoute("#/nada", ids)).toBe("aportar");
    expect(parseRoute("", ids)).toBe("aportar");
  });

  it("os links antigos levam à seção certa de Ajustes", () => {
    expect(parseRoute("#/metas", ids)).toBe("ajustes");
    expect(parseSection("#/metas")).toBe("metas");
    expect(parseRoute("#criterios", ids)).toBe("ajustes");
    expect(parseSection("#/criterios?x=1")).toBe("criterios");
    expect(hashSegments("#/ajustes/metas")).toEqual(["ajustes", "metas"]);
  });

  it("a seção que mudou de nome segue valendo: Saúde da carteira virou Análise", () => {
    expect(hashSegments("#/ajustes/saude")).toEqual(["ajustes", "analise"]);
    expect(parseRoute("#/ajustes/saude", ids)).toBe("ajustes");
    expect(parseSection("#/ajustes/saude?x=1")).toBe("analise");
    // O endereço novo não é traduzido de novo.
    expect(hashSegments("#/ajustes/analise")).toEqual(["ajustes", "analise"]);
    // "saude" fora de Ajustes não é um endereço que existiu.
    expect(hashSegments("#/saude")).toEqual(["saude"]);
  });

  it("a seção é o segundo trecho, e só ele", () => {
    expect(parseSection("#/ajustes")).toBeNull();
    expect(parseSection("#/ajustes/criterios")).toBe("criterios");
    expect(parseSection("")).toBeNull();
  });

  it("navega escrevendo o hash", () => {
    navigate("carteira");
    expect(window.location.hash).toBe("#/carteira");
    navigate("ajustes/metas");
    expect(window.location.hash).toBe("#/ajustes/metas");
  });
});

describe("assetClasses", () => {
  it("adivinha a classe pelo ticker quando dá", () => {
    expect(guessClassFromSymbol("knri11")).toBe("br_reits");
    expect(guessClassFromSymbol("PETR4.SA")).toBe("br_stocks");
    expect(guessClassFromSymbol("btc")).toBe("crypto");
    expect(guessClassFromSymbol("VOO")).toBeNull();
  });

  it("sabe quem é por valor e a cor de cada classe", () => {
    expect(isValueBased("fixed_income")).toBe(true);
    expect(isValueBased("br_reits")).toBe(false);
    expect(classColor("crypto")).toBe("var(--c-crypto)");
    expect(CLASS_META.br_reits.noun).toEqual(["cota", "cotas"]);
  });

  it("um ativo da B3 é cota inteira em qualquer classe", () => {
    expect(acceptsB3Listing("br_stocks")).toBe(false);
    expect(acceptsB3Listing("br_reits")).toBe(false);
    expect(acceptsB3Listing("crypto")).toBe(true);
    expect(acceptsB3Listing("fixed_income")).toBe(true);
    expect(unitOf("crypto", false)).toBe("fraction");
    expect(unitOf("crypto", true)).toBe("unit");
    expect(unitOf("fixed_income", false)).toBe("value");
    expect(unitOf("fixed_income", true)).toBe("unit");
    expect(nounOf("crypto", true)).toEqual(["cota", "cotas"]);
    expect(nounOf("crypto", false)).toEqual(["unidade", "unidades"]);
    expect(nounOf("br_stocks", true)).toEqual(["ação", "ações"]);
  });

  it("reconhece o formato de ticker da B3", () => {
    expect(looksLikeB3Ticker("qbtc11")).toBe(true);
    expect(looksLikeB3Ticker("AAPL34")).toBe(true);
    expect(looksLikeB3Ticker("IVVB11.SA")).toBe(true);
    expect(looksLikeB3Ticker("PETR4")).toBe(true);
    expect(looksLikeB3Ticker("BTC")).toBe(false);
    expect(looksLikeB3Ticker("VOO")).toBe(false);
    expect(looksLikeB3Ticker("AAPL")).toBe(false);
  });
});

describe("planText", () => {
  const shown = createMoney(false);

  it("escreve cada aviso numa frase que diz o que fazer", () => {
    expect(nbsp(noticeText({ code: "class_blocked", asset_class: "intl_stocks", amount: 25226.45, reason: "no_assets" }, [], shown))).toContain(
      "Ações Internacionais precisa de R$ 25,2 mil",
    );
    expect(noticeText({ code: "assets_without_price", symbols: ["XYZW11"] }, [], shown)).toContain("XYZW11");
    expect(noticeText({ code: "assets_without_score", symbols: ["MXRF11", "ITSA4"] }, [], shown)).toContain("MXRF11, ITSA4");
    expect(noticeText({ code: "no_targets" }, [], shown)).toContain("metas");
  });

  it("a classe bloqueada diz o que falta nela", () => {
    const blocked = (reason: "no_assets" | "unrated" | "unpriced") =>
      noticeText({ code: "class_blocked", asset_class: "br_reits", amount: 499600, reason }, [], shown);
    expect(blocked("no_assets")).toContain("ainda não tem nenhum ativo");
    expect(blocked("unrated")).toContain("nenhum ativo dela tem nota");
    expect(blocked("unpriced")).toContain("sem cotação");
  });

  it("ticker da B3 sem cotação numa classe de fora sugere ligar a B3", () => {
    const etf = {
      asset_class: "crypto" as const,
      symbol: "QBTC11",
      display_name: "QR Bitcoin",
      listed_on_b3: false,
    };
    expect(looksMissingB3Flag(etf)).toBe(true);
    expect(looksMissingB3Flag({ ...etf, listed_on_b3: true })).toBe(false);
    expect(looksMissingB3Flag({ ...etf, symbol: "BTC" })).toBe(false);
    expect(looksMissingB3Flag({ ...etf, asset_class: "br_stocks" })).toBe(false);
    const asset = { ...toAsset(must(defaultAssets()[0]), defaultChecklists(), 0), ...etf };
    const one = noticeText({ code: "assets_without_price", symbols: ["QBTC11"] }, [asset], shown);
    expect(one).toContain("Fica fora da conta");
    expect(one).toContain("Negociado na B3");
    const many = noticeText({ code: "assets_without_price", symbols: ["SUMIU3", "XYZW11"] }, [asset], shown);
    expect(many).toContain("Ficam fora da conta");
    expect(many).not.toContain("Negociado na B3");
  });

  it("título do Tesouro é fração de título, com nome e não com ticker", () => {
    expect(unitOf("fixed_income", false)).toBe("value");
    expect(unitOf("fixed_income", false, true)).toBe("fraction");
    expect(nounOf("fixed_income", false, true)).toEqual(["título", "títulos"]);
    expect(isTreasuryKey("TD-IPCA-20350515")).toBe(true);
    expect(isTreasuryKey("TD-PREJS-20350101")).toBe(true);
    for (const other of ["KNRI11", "TD-IPCA", "TD-IPCA-2035051", "td-ipca-20350515", "", null, undefined]) {
      expect(isTreasuryKey(other)).toBe(false);
    }
    expect(assetName({ symbol: "TD-IPCA-20350515", display_name: "Tesouro IPCA+ 2035" })).toBe("Tesouro IPCA+ 2035");
    expect(assetName({ symbol: "KNRI11", display_name: "Kinea" })).toBe("KNRI11");
    expect(assetName({ symbol: null, display_name: "CDB Banco X" })).toBe("CDB Banco X");
    expect(formatDay("2026-11-19T03:00:00Z")).toBe("19/11/2026");
    // A meia-noite de Brasília ainda é o dia anterior em UTC: o dia certo é o de Brasília.
    expect(formatDay("2026-11-20T02:59:00Z")).toBe("19/11/2026");
    expect(formatDay("lixo")).toBe("");
    const bond = { ...must(samplePlan().items[0]), asset_class: "fixed_income" as const, symbol: "TD-IPCA-20350515", display_name: "Tesouro IPCA+ 2035", listed_on_b3: false, treasury: true, divisible: true };
    expect(nbsp(unitLine({ ...bond, quantity: 0.39, unit_price: 2500.95 }, shown))).toBe("0,39 títulos × R$ 2.500,95");
    expect(nbsp(unitLine({ ...bond, quantity: 1, unit_price: 2500.95 }, shown))).toBe("1 título × R$ 2.500,95");
    // Olho fechado: quantidade e preço mascarados, a unidade (o tipo de ativo) continua legível.
    expect(unitLine({ ...bond, quantity: 0.39, unit_price: 2500.95 }, createMoney(true))).toBe(`${MASK_QUANTITY} títulos × ${MASK_BRL}`);
  });

  it("descreve a compra de cada item", () => {
    const items = samplePlan().items;
    const btlg = must(items[1]);
    const btc = must(items[3]);
    expect(nbsp(unitLine(btlg, shown))).toBe("8 cotas × R$ 94,55");
    expect(nbsp(unitLine(btc, shown))).toBe("0,0045 BTC × R$ 345.588,05");
    expect(
      unitLine({ ...btlg, asset_class: "fixed_income", listed_on_b3: false, symbol: null, quantity: 100, unit_price: 1, value: 100 }, shown),
    ).toBe("Aplicar este valor");
    // ETF de bitcoin da B3 dentro de Cripto: cota inteira, não fração de BTC.
    expect(nbsp(unitLine({ ...btc, symbol: "QBTC11", listed_on_b3: true, quantity: 5, unit_price: 52 }, shown))).toBe(
      "5 cotas × R$ 52,00",
    );
  });

  it("com os valores ocultos, nenhuma frase carrega número", () => {
    const hidden = createMoney(true);
    const items = samplePlan().items;
    const line = unitLine(must(items[3]), hidden);
    expect(line).toContain(MASK_QUANTITY);
    expect(line).toContain(MASK_BRL);
    expect(line).not.toMatch(/\d/);
    const blocked = noticeText(
      { code: "class_blocked", asset_class: "intl_stocks", amount: 25226.45, reason: "no_assets" },
      [],
      hidden,
    );
    expect(blocked).toContain(MASK_BRL);
    expect(blocked).not.toMatch(/\d/);
  });
});

describe("privacidade dos valores", () => {
  it("o formatador oculto devolve só a máscara, o visível o número", () => {
    const hidden = createMoney(true);
    expect(hidden.hidden).toBe(true);
    expect([hidden.brl(1234.5), hidden.compact(136000)]).toEqual([MASK_BRL, MASK_BRL]);
    expect(hidden.quantity(12, false)).toBe(MASK_QUANTITY);
    expect(nbsp(createMoney(false).brl(1234.5))).toBe("R$ 1.234,50");
    expect(createMoney(false).hidden).toBe(false);
  });

  it("a escolha vai para o localStorage e volta de lá", () => {
    expect(readHideValues()).toBe(false);
    saveHideValues(true);
    expect(window.localStorage.getItem(HIDE_VALUES_KEY)).toBe("1");
    expect(readHideValues()).toBe(true);
    saveHideValues(false);
    expect(window.localStorage.getItem(HIDE_VALUES_KEY)).toBeNull();
    expect(readHideValues()).toBe(false);
  });

  it("armazenamento bloqueado não derruba: lê como visível e grava sem erro", () => {
    const blocked = vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new Error("bloqueado");
    });
    const write = vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("bloqueado");
    });
    expect(readHideValues()).toBe(false);
    expect(() => saveHideValues(true)).not.toThrow();
    blocked.mockRestore();
    write.mockRestore();
  });
});

describe("celebrate", () => {
  afterEach(() => vi.restoreAllMocks());

  it("solta confete dourado", () => {
    vi.mocked(confetti).mockClear();
    expect(celebrate({ x: 0.2, y: 0.4 })).toBe(true);
    expect(confetti).toHaveBeenCalledTimes(2);
    expect(vi.mocked(confetti).mock.calls[0]?.[0]).toMatchObject({ origin: { x: 0.2, y: 0.4 } });
  });

  it("não dispara com movimento reduzido", () => {
    vi.spyOn(window, "matchMedia").mockReturnValue({ matches: true } as MediaQueryList);
    vi.mocked(confetti).mockClear();
    expect(prefersReducedMotion()).toBe(true);
    expect(celebrate()).toBe(false);
    expect(confetti).not.toHaveBeenCalled();
  });

  it("calcula a origem a partir de um elemento", () => {
    expect(originFrom(null)).toBeUndefined();
    const el = document.createElement("div");
    vi.spyOn(el, "getBoundingClientRect").mockReturnValue({ left: 100, top: 200, width: 50, height: 20 } as DOMRect);
    const origin = originFrom(el);
    expect(origin?.x).toBeCloseTo(125 / window.innerWidth);
    expect(origin?.y).toBeCloseTo(210 / window.innerHeight);
  });
});

describe("cx", () => {
  it("junta só o que é verdade", () => {
    expect(cx("a", false, null, undefined, "b")).toBe("a b");
  });
});
