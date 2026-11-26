import { fireEvent, screen, waitFor } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";
import { setIcons } from "../test/iconsHandlers";
import { renderWithProviders } from "../test/render";
import { server } from "../test/server";
import { AssetIcon } from "./AssetIcon";

const ITSA4 = { symbol: "ITSA4", version: "9a3f6c0de2" };

const icon = (container: HTMLElement) => container.querySelector<HTMLElement>(".asset-icon");
const image = (container: HTMLElement) => container.querySelector<HTMLImageElement>("img");

describe("AssetIcon", () => {
  it("mostra o logo do domínio do Prumo, sem alt, preguiçoso e com a versão na URL", async () => {
    setIcons([ITSA4]);
    const { container } = renderWithProviders(<AssetIcon symbol="ITSA4" name="Itaúsa" assetClass="br_stocks" />);
    await waitFor(() => expect(image(container)).not.toBeNull());
    const img = image(container);
    expect(img).toHaveAttribute("src", "/api/v1/icons/ITSA4?v=9a3f6c0de2");
    expect(img).toHaveAttribute("alt", "");
    expect(img).toHaveAttribute("loading", "lazy");
    // É enfeite: o nome do ativo está ao lado, e nada do ícone vai ao leitor de tela.
    expect(icon(container)).toHaveAttribute("aria-hidden", "true");
    expect(screen.queryByRole("img")).not.toBeInTheDocument();
  });

  it("não pisca: as iniciais ficam por baixo até a imagem carregar", async () => {
    setIcons([ITSA4]);
    const { container } = renderWithProviders(<AssetIcon symbol="ITSA4" assetClass="br_stocks" />);
    await waitFor(() => expect(image(container)).not.toBeNull());
    // Antes do `load`: a imagem existe mas ainda não aparece, e as iniciais estão lá.
    expect(image(container)).not.toHaveAttribute("data-loaded");
    expect(container).toHaveTextContent("IT");
    fireEvent.load(image(container) as HTMLImageElement);
    expect(image(container)).toHaveAttribute("data-loaded");
  });

  it("se a imagem falha, sai e ficam as iniciais", async () => {
    setIcons([ITSA4]);
    const { container } = renderWithProviders(<AssetIcon symbol="ITSA4" assetClass="br_stocks" />);
    await waitFor(() => expect(image(container)).not.toBeNull());
    fireEvent.error(image(container) as HTMLImageElement);
    expect(image(container)).toBeNull();
    expect(container).toHaveTextContent("IT");
  });

  it("uma versão nova do logo tem nova chance depois de um erro", async () => {
    setIcons([ITSA4]);
    const { container, client } = renderWithProviders(<AssetIcon symbol="ITSA4" assetClass="br_stocks" />);
    await waitFor(() => expect(image(container)).not.toBeNull());
    fireEvent.error(image(container) as HTMLImageElement);
    expect(image(container)).toBeNull();

    setIcons([{ symbol: "ITSA4", version: "novaversao1" }]);
    await client.invalidateQueries({ queryKey: ["icons"] });
    await waitFor(() => expect(image(container)).toHaveAttribute("src", "/api/v1/icons/ITSA4?v=novaversao1"));
  });

  it("sem logo guardado, mostra as iniciais e não pede imagem nenhuma", async () => {
    setIcons([ITSA4]);
    const { container } = renderWithProviders(<AssetIcon symbol="KNRI11" assetClass="br_reits" />);
    await waitFor(() => expect(container).toHaveTextContent("KN"));
    expect(image(container)).toBeNull();
    expect(icon(container)).toHaveAttribute("data-kind", "initials");
  });

  it("acha o logo pelo ticker em qualquer caixa", async () => {
    setIcons([ITSA4]);
    const { container } = renderWithProviders(<AssetIcon symbol="itsa4" />);
    await waitFor(() => expect(image(container)).not.toBeNull());
  });

  it("a lista de logos fora do ar só quer dizer 'sem logos'", async () => {
    server.use(http.get("/api/v1/icons", () => HttpResponse.json({ code: "internal", status: 500 }, { status: 500 })));
    const { container } = renderWithProviders(<AssetIcon symbol="ITSA4" assetClass="br_stocks" />);
    await waitFor(() => expect(container).toHaveTextContent("IT"));
    expect(image(container)).toBeNull();
  });

  it("o Tesouro tem ícone próprio, sem imagem, mesmo que a chave estivesse na lista", () => {
    setIcons([{ symbol: "TD-IPCA-20350515", version: "x" }]);
    const { container } = renderWithProviders(<AssetIcon symbol="TD-IPCA-20350515" name="Tesouro IPCA+ 2035" treasury />);
    expect(icon(container)).toHaveAttribute("data-kind", "treasury");
    expect(container.querySelector("svg.asset-icon__glyph")).not.toBeNull();
    expect(image(container)).toBeNull();
    expect(container).not.toHaveTextContent("TD");
  });

  it("bitcoin e cripto genérica têm desenhos diferentes, na cor de cripto", () => {
    const bitcoin = renderWithProviders(<AssetIcon symbol="BTC" assetClass="crypto" />);
    expect(icon(bitcoin.container)).toHaveAttribute("data-kind", "bitcoin");
    const bitcoinPath = bitcoin.container.querySelector("svg path")?.getAttribute("d");
    bitcoin.unmount();

    const ether = renderWithProviders(<AssetIcon symbol="ETH" assetClass="crypto" />);
    expect(icon(ether.container)).toHaveAttribute("data-kind", "crypto");
    expect(ether.container.querySelector("svg circle")).not.toBeNull();
    expect(ether.container.querySelector("svg path")?.getAttribute("d")).not.toBe(bitcoinPath);
    expect(icon(ether.container)?.style.getPropertyValue("--icon-bg")).toBe("var(--c-crypto)");
  });

  it("um ETF de bitcoin da B3 com logo mostra o logo, não o desenho de cripto", async () => {
    setIcons([{ symbol: "QBTC11", version: "q1" }]);
    const { container } = renderWithProviders(<AssetIcon symbol="QBTC11" assetClass="crypto" />);
    await waitFor(() => expect(image(container)).toHaveAttribute("src", "/api/v1/icons/QBTC11?v=q1"));
  });

  it("renda fixa por valor usa as iniciais do nome e a cor da classe", () => {
    const { container } = renderWithProviders(<AssetIcon symbol={null} name="Caixa Reserva" assetClass="fixed_income" />);
    expect(container).toHaveTextContent("CR");
    expect(icon(container)?.style.getPropertyValue("--icon-bg")).toBe("var(--c-fixed_income)");
  });

  it("sem classe nem palpite, o círculo é neutro", () => {
    const { container } = renderWithProviders(<AssetIcon symbol="VOO" />);
    expect(icon(container)?.style.getPropertyValue("--icon-bg")).toBe("var(--text-muted)");
  });

  it("o tamanho é uma das três medidas do desenho", () => {
    const { container } = renderWithProviders(<AssetIcon symbol="ITSA4" size="sm" className="extra" />);
    expect(icon(container)).toHaveClass("asset-icon--sm", "extra");
  });
});
