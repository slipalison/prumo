import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { renderWithProviders } from "../test/render";
import { AnswerToggle } from "./AnswerToggle";
import { AuroraBackground } from "./AuroraBackground";
import { ClassBar } from "./ClassBar";
import { CountUp } from "./CountUp";
import { Donut } from "./Donut";
import { GlassCard } from "./GlassCard";
import { MagneticButton } from "./MagneticButton";
import { MoneyInput } from "./MoneyInput";
import { Plumb } from "./Plumb";
import { QuantityInput } from "./QuantityInput";
import { ScoreBadge } from "./ScoreBadge";
import { ScoreSlider } from "./ScoreSlider";
import { Sheet } from "./Sheet";
import { ThemeToggle } from "./ThemeToggle";
import { useToast } from "./toastContext";
import type { Answer } from "../lib/score";

const nbsp = (text: string | null | undefined) => (text ?? "").replace(/\u00a0/g, " ");

describe("visuais", () => {
  it("a aurora é decorativa", () => {
    const { container } = render(<AuroraBackground />);
    expect(container.firstElementChild).toHaveAttribute("aria-hidden", "true");
    expect(container.querySelectorAll(".aurora__blob")).toHaveLength(4);
  });

  it("o vidro acompanha o ponteiro com variáveis CSS", () => {
    const onMove = vi.fn();
    render(
      <GlassCard data-testid="card" onPointerMove={onMove}>
        oi
      </GlassCard>,
    );
    const card = screen.getByTestId("card");
    fireEvent.pointerMove(card, { clientX: 30, clientY: 40 });
    expect(card.style.getPropertyValue("--mx")).toMatch(/px$/);
    expect(onMove).toHaveBeenCalled();
  });

  it("vidro sem holofote não escreve as variáveis", () => {
    render(
      <GlassCard as="section" spotlight={false} aria-label="seção">
        oi
      </GlassCard>,
    );
    const card = screen.getByRole("region", { name: "seção" });
    fireEvent.pointerMove(card, { clientX: 30, clientY: 40 });
    expect(card.style.getPropertyValue("--mx")).toBe("");
    expect(card).not.toHaveClass("glass--spotlight");
  });

  it.each(["idle", "swinging", "settled", "success"] as const)("o prumo desenha o estado %s", (state) => {
    const { container } = render(<Plumb state={state} label={`prumo ${state}`} />);
    expect(screen.getByRole("img", { name: `prumo ${state}` })).toBeInTheDocument();
    expect(container.querySelector(".plumb__check") !== null).toBe(state === "success");
  });

  it("a rosca desenha só os segmentos com valor e descreve o todo", () => {
    const { container } = render(
      <Donut
        label="Carteira por classe"
        segments={[
          { key: "a", value: 60, color: "red", label: "A" },
          { key: "b", value: 40, color: "blue", label: "B" },
          { key: "c", value: 0, color: "green", label: "C" },
        ]}
      >
        <span>centro</span>
      </Donut>,
    );
    expect(screen.getByRole("img", { name: "Carteira por classe" })).toBeInTheDocument();
    expect(container.querySelectorAll("[data-segment]")).toHaveLength(2);
    expect(screen.getByText("centro")).toBeInTheDocument();
  });

  it("rosca vazia mostra só o trilho", () => {
    const { container } = render(<Donut label="vazia" segments={[]} />);
    expect(container.querySelectorAll("[data-segment]")).toHaveLength(0);
    expect(container.querySelector(".donut__track")).not.toBeNull();
  });

  it("a barra de classe mostra meta, atual e o pedaço do aporte", () => {
    const { container, rerender } = render(<ClassBar assetClass="br_reits" current={5.36} target={10} after={6.43} />);
    expect(screen.getByRole("meter")).toHaveAccessibleName(/Fundos Imobiliários: 5,4% hoje, 6,4% depois do aporte, meta 10,0%/);
    expect(container.querySelector(".class-bar__added")).not.toBeNull();
    expect(container.querySelector(".class-bar__target")).not.toBeNull();
    rerender(<ClassBar assetClass="crypto" current={0} target={0} />);
    expect(screen.getByRole("meter")).toHaveAccessibleName("Criptomoedas: 0,0% de 0,0% da meta");
    expect(container.querySelector(".class-bar__added")).toBeNull();
    expect(container.querySelector(".class-bar__target")).toBeNull();
  });

  it("o contador chega ao valor final", async () => {
    const { rerender } = render(<CountUp value={6000} />);
    await waitFor(() => expect(nbsp(screen.getByText(/6\.000/).textContent)).toBe("R$ 6.000,00"));
    rerender(<CountUp value={1234.5} format={(v) => `${v.toFixed(1)} un`} />);
    await waitFor(() => expect(screen.getByText("1234.5 un")).toBeInTheDocument());
  });

  it("a nota mostra o número e o tom", () => {
    const { container, rerender } = render(<ScoreBadge score={0} />);
    expect(container.firstElementChild).toHaveClass("score--zero");
    rerender(<ScoreBadge score={2} />);
    expect(container.firstElementChild).toHaveClass("score--low");
    rerender(<ScoreBadge score={5} size="lg" />);
    expect(container.firstElementChild).toHaveClass("score--mid", "score--lg");
    rerender(<ScoreBadge score={9} />);
    expect(container.firstElementChild).toHaveClass("score--high");
    expect(screen.getByText("9")).toBeInTheDocument();
  });
});

describe("controles", () => {
  it("botão magnético chama o clique, segue o ponteiro e trava carregando", async () => {
    const onClick = vi.fn();
    const onMove = vi.fn();
    const onLeave = vi.fn();
    const user = userEvent.setup();
    const { rerender } = render(
      <MagneticButton onClick={onClick} onPointerMove={onMove} onPointerLeave={onLeave}>
        Calcular
      </MagneticButton>,
    );
    const button = screen.getByRole("button", { name: "Calcular" });
    fireEvent.pointerMove(button, { clientX: 10, clientY: 10 });
    fireEvent.pointerLeave(button);
    expect(onMove).toHaveBeenCalled();
    expect(onLeave).toHaveBeenCalled();
    await user.click(button);
    expect(onClick).toHaveBeenCalledTimes(1);
    rerender(
      <MagneticButton onClick={onClick} loading variant="ghost">
        Calcular
      </MagneticButton>,
    );
    expect(screen.getByRole("button", { name: "Calcular" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Calcular" })).toHaveAttribute("aria-busy", "true");
  });

  it("sim/não: marca, troca e desfaz", async () => {
    const user = userEvent.setup();
    function Harness() {
      const [value, setValue] = useState<Answer>(null);
      return (
        <>
          <AnswerToggle questionId="q1" prompt="Paga dividendos?" value={value} onChange={setValue} />
          <output>{String(value)}</output>
        </>
      );
    }
    render(<Harness />);
    const group = screen.getByRole("group", { name: "Paga dividendos?" });
    const yes = within(group).getByRole("button", { name: "Sim" });
    const no = within(group).getByRole("button", { name: "Não" });
    await user.click(yes);
    expect(yes).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByText("true")).toBeInTheDocument();
    await user.click(no);
    expect(screen.getByText("false")).toBeInTheDocument();
    await user.click(no);
    expect(screen.getByText("null")).toBeInTheDocument();
  });

  it("campo de dinheiro aplica a máscara e responde ao Enter", async () => {
    const user = userEvent.setup();
    const onEnter = vi.fn();
    function Harness() {
      const [value, setValue] = useState(0);
      return <MoneyInput id="m" label="Valor" value={value} onChange={setValue} onEnter={onEnter} focusOnMount />;
    }
    render(<Harness />);
    const input = screen.getByLabelText("Valor");
    expect(input).toHaveFocus();
    await user.type(input, "600000");
    expect(nbsp((input as HTMLInputElement).value)).toBe("R$ 6.000,00");
    await user.type(input, "{Backspace}");
    expect(nbsp((input as HTMLInputElement).value)).toBe("R$ 600,00");
    await user.keyboard("{Enter}");
    expect(onEnter).toHaveBeenCalled();
  });

  it("dinheiro: o foco seleciona tudo, e digitar começa um valor novo", async () => {
    const user = userEvent.setup();
    function Harness() {
      const [value, setValue] = useState(3000);
      return (
        <>
          <MoneyInput id="m" label="Valor" value={value} onChange={setValue} />
          <button type="button">fora</button>
        </>
      );
    }
    render(<Harness />);
    const input = screen.getByLabelText<HTMLInputElement>("Valor");
    await user.click(input);
    expect(input.selectionStart).toBe(0);
    expect(input.selectionEnd).toBe(input.value.length);
    await user.keyboard("300000");
    expect(nbsp(input.value)).toBe("R$ 3.000,00");
    // Com o campo já focado, o clique não reposiciona o cursor no meio do número.
    expect(fireEvent.mouseDown(input)).toBe(false);
    fireEvent.mouseUp(input);
    await user.click(screen.getByRole("button", { name: "fora" }));
    expect(fireEvent.mouseDown(input)).toBe(true);
    fireEvent.focus(input);
    expect(input.selectionEnd).toBe(input.value.length);
  });

  it("dinheiro: com o campo vazio o foco não seleciona o zero (sem bloco dourado) e o dígito entra como centavo", async () => {
    const user = userEvent.setup();
    function Harness() {
      const [value, setValue] = useState(0);
      return <MoneyInput id="m" label="Valor" value={value} onChange={setValue} />;
    }
    render(<Harness />);
    const input = screen.getByLabelText<HTMLInputElement>("Valor");
    await user.click(input);
    // Nada a substituir: o cursor fica no fim, sem seleção cobrindo "R$ 0,00".
    expect(input.selectionStart).toBe(input.value.length);
    expect(input.selectionEnd).toBe(input.value.length);
    await user.keyboard("5");
    expect(nbsp(input.value)).toBe("R$ 0,05");
    await user.keyboard("00");
    expect(nbsp(input.value)).toBe("R$ 5,00");
  });

  it("dinheiro: digitar logo depois do foco cancela a seleção atrasada", () => {
    vi.useFakeTimers();
    try {
      function Harness() {
        const [value, setValue] = useState(5000);
        return <MoneyInput id="m" label="Valor" value={value} onChange={setValue} />;
      }
      render(<Harness />);
      const input = screen.getByLabelText<HTMLInputElement>("Valor");
      act(() => {
        input.focus();
      });
      fireEvent.keyDown(input, { key: "1" });
      fireEvent.change(input, { target: { value: "R$ 5.000,001" } });
      act(() => {
        vi.runAllTimers();
      });
      // A seleção atrasada foi cancelada: o cursor não volta a cobrir o valor.
      expect(input.selectionStart).toBe(input.selectionEnd);
    } finally {
      vi.useRealTimers();
    }
  });

  it("dinheiro: valor longo encolhe a fonte, curto fica cheio", () => {
    const { rerender } = render(<MoneyInput id="m" label="Valor" value={60} onChange={() => {}} />);
    const input = screen.getByLabelText("Valor");
    expect(input.style.getPropertyValue("--money-fit")).toBe("1.000");
    rerender(<MoneyInput id="m" label="Valor" value={3_000_003_000} onChange={() => {}} />);
    expect(Number(input.style.getPropertyValue("--money-fit"))).toBeLessThan(1);
  });

  it("quantidade inteira com − e +, e digitação", async () => {
    const user = userEvent.setup();
    function Harness() {
      const [value, setValue] = useState(2);
      return (
        <>
          <QuantityInput id="q" label="Cotas" value={value} onChange={setValue} />
          <output>{value}</output>
        </>
      );
    }
    render(<Harness />);
    await user.click(screen.getByRole("button", { name: "Aumentar cotas" }));
    expect(screen.getByRole("status")).toHaveTextContent("3");
    await user.click(screen.getByRole("button", { name: "Diminuir cotas" }));
    await user.click(screen.getByRole("button", { name: "Diminuir cotas" }));
    await user.click(screen.getByRole("button", { name: "Diminuir cotas" }));
    expect(screen.getByRole("status")).toHaveTextContent("0");
    expect(screen.getByRole("button", { name: "Diminuir cotas" })).toBeDisabled();
    const input = screen.getByLabelText("Cotas");
    await user.clear(input);
    await user.type(input, "1.500");
    expect(screen.getByRole("status")).toHaveTextContent("1500");
    await user.tab();
    expect((input as HTMLInputElement).value).toBe("1.500");
  });

  it("quantidade fracionada aceita vírgula", async () => {
    const user = userEvent.setup();
    function Harness() {
      const [value, setValue] = useState(0);
      return (
        <>
          <QuantityInput id="f" label="Quantidade" value={value} onChange={setValue} fractional />
          <output>{value}</output>
        </>
      );
    }
    render(<Harness />);
    expect(screen.queryByRole("button", { name: /Aumentar/ })).toBeNull();
    const input = screen.getByLabelText("Quantidade");
    await user.clear(input);
    await user.type(input, "0,015");
    expect(screen.getByRole("status")).toHaveTextContent("0.015");
    expect((input as HTMLInputElement).value).toBe("0,015");
  });

  it("nota manual de 0 a 10", () => {
    const onChange = vi.fn();
    render(<ScoreSlider id="s" value={5} onChange={onChange} hint="dica" />);
    fireEvent.change(screen.getByLabelText("Nota de confiança (0 a 10)"), { target: { value: "8" } });
    expect(onChange).toHaveBeenCalledWith(8);
    expect(screen.getByText("dica")).toBeInTheDocument();
  });

  it("o tema troca escuro → claro → sistema → escuro", async () => {
    const user = userEvent.setup();
    render(<ThemeToggle />);
    const button = screen.getByRole("button", { name: /Tema escuro/ });
    await user.click(button);
    expect(document.documentElement.dataset.theme).toBe("light");
    expect(screen.getByRole("button", { name: /Tema claro/ })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /Tema claro/ }));
    expect(document.documentElement.hasAttribute("data-theme")).toBe(false);
    await user.click(screen.getByRole("button", { name: /Tema do sistema/ }));
    expect(document.documentElement.dataset.theme).toBe("dark");
    expect(window.localStorage.getItem("prumo.theme")).toBe("dark");
  });
});

describe("Sheet", () => {
  function Harness({ onClose = vi.fn() }: { onClose?: () => void }) {
    const [open, setOpen] = useState(false);
    return (
      <>
        <button type="button" onClick={() => setOpen(true)}>
          abrir
        </button>
        <Sheet
          open={open}
          onClose={() => {
            onClose();
            setOpen(false);
          }}
          title="Novo ativo"
          description="Digite o ticker"
          footer={<span>rodapé</span>}
        >
          <input aria-label="primeiro" />
          <input aria-label="foco" data-autofocus="" />
          <button type="button">último</button>
        </Sheet>
      </>
    );
  }

  it("abre com foco no campo marcado, prende o Tab e devolve o foco ao fechar com Esc", async () => {
    const user = userEvent.setup();
    render(<Harness />);
    const opener = screen.getByRole("button", { name: "abrir" });
    await user.click(opener);
    const dialog = screen.getByRole("dialog", { name: "Novo ativo" });
    expect(dialog).toHaveAccessibleDescription("Digite o ticker");
    expect(screen.getByLabelText("foco")).toHaveFocus();
    expect(document.body.style.overflow).toBe("hidden");
    expect(screen.getByText("rodapé")).toBeInTheDocument();

    screen.getByRole("button", { name: "último" }).focus();
    await user.tab();
    expect(within(dialog).getAllByRole("button", { name: "Fechar" })[0]).toHaveFocus();
    await user.tab({ shift: true });
    expect(screen.getByRole("button", { name: "último" })).toHaveFocus();

    await user.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(opener).toHaveFocus();
    expect(document.body.style.overflow).toBe("");
  });

  it("fecha pelo botão e pelo fundo", async () => {
    const user = userEvent.setup();
    const onClose = vi.fn();
    render(<Harness onClose={onClose} />);
    await user.click(screen.getByRole("button", { name: "abrir" }));
    const dialog = screen.getByRole("dialog");
    await user.click(within(dialog).getByRole("button", { name: "Fechar" }));
    expect(onClose).toHaveBeenCalledTimes(1);
    await user.click(screen.getByRole("button", { name: "abrir" }));
    const backdrop = document.querySelector(".sheet__backdrop");
    expect(backdrop).not.toBeNull();
    fireEvent.click(backdrop as Element);
    expect(onClose).toHaveBeenCalledTimes(2);
  });

  it("com nada focável, o Tab fica no painel", async () => {
    const user = userEvent.setup();
    render(
      <Sheet open onClose={() => undefined} title="Vazio">
        <p>só texto</p>
      </Sheet>,
    );
    const close = within(screen.getByRole("dialog")).getByRole("button", { name: "Fechar" });
    expect(close).toHaveFocus();
    await user.tab();
    expect(close).toHaveFocus();
  });
});

describe("toasts", () => {
  afterEach(() => vi.useRealTimers());

  function Trigger() {
    const toast = useToast();
    return (
      <>
        <button type="button" onClick={() => toast({ tone: "success", message: "Aporte registrado." })}>
          ok
        </button>
        <button type="button" onClick={() => toast({ tone: "error", message: "Falhou." })}>
          erro
        </button>
        <button type="button" onClick={() => toast({ tone: "info", message: "Info." })}>
          info
        </button>
      </>
    );
  }

  it("aparece e pode ser dispensado", async () => {
    const { user } = renderWithProviders(<Trigger />);
    await user.click(screen.getByRole("button", { name: "ok" }));
    expect(await screen.findByText("Aporte registrado.")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "erro" }));
    await user.click(screen.getByRole("button", { name: "info" }));
    expect(screen.getByText("Falhou.")).toBeInTheDocument();
    await user.click(screen.getAllByRole("button", { name: "Dispensar aviso" })[0] as HTMLElement);
    // A saída do AnimatePresence roda em tempo real no happy-dom (~1 s).
    await waitFor(() => expect(screen.queryByText("Aporte registrado.")).toBeNull(), { timeout: 3000 });
    expect(screen.getByText("Info.")).toBeInTheDocument();
  });

  it("some sozinho depois de alguns segundos", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    renderWithProviders(<Trigger />);
    fireEvent.click(screen.getByRole("button", { name: "info" }));
    expect(screen.getByText("Info.")).toBeInTheDocument();
    act(() => {
      vi.advanceTimersByTime(4600);
    });
    vi.useRealTimers();
    await waitFor(() => expect(screen.queryByText("Info.")).toBeNull(), { timeout: 3000 });
  });
});
