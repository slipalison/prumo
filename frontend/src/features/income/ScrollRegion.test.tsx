import { render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { ScrollRegion } from "./ScrollRegion";

/** O jsdom não tem layout: as medidas da caixa e do conteúdo vêm daqui. */
function fakeSizes(content: number, box: number) {
  const proto = HTMLElement.prototype;
  Object.defineProperty(proto, "scrollHeight", { configurable: true, get: () => content });
  Object.defineProperty(proto, "clientHeight", { configurable: true, get: () => box });
}

afterEach(() => {
  delete (HTMLElement.prototype as Partial<Record<"scrollHeight" | "clientHeight", unknown>>).scrollHeight;
  delete (HTMLElement.prototype as Partial<Record<"scrollHeight" | "clientHeight", unknown>>).clientHeight;
});

describe("ScrollRegion", () => {
  it("quando rola, vira região com nome e entra no Tab (o Safari não faz isso sozinho)", () => {
    fakeSizes(900, 300);
    render(
      <ScrollRegion className="lista" label="Próximos pagamentos, lista com rolagem">
        <p>conteúdo</p>
      </ScrollRegion>,
    );
    const region = screen.getByRole("region", { name: "Próximos pagamentos, lista com rolagem" });
    expect(region).toHaveAttribute("tabindex", "0");
    expect(region).toHaveClass("lista");
  });

  it("quando cabe, é uma caixa comum, sem parada de foco à toa", () => {
    fakeSizes(200, 300);
    const { container } = render(
      <ScrollRegion className="lista" label="Próximos pagamentos, lista com rolagem">
        <p>conteúdo</p>
      </ScrollRegion>,
    );
    expect(screen.queryByRole("region")).not.toBeInTheDocument();
    expect(container.querySelector(".lista")).not.toHaveAttribute("tabindex");
  });
});
