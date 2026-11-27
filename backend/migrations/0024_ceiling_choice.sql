-- Teto de preço por escolha (docs/alocacao.md § Teto de preço por ativo).
--
-- O teto de um ativo passa a ser uma de quatro escolhas da pessoa: nenhum,
-- um método de referência (o valor recalcula a cada leitura), ou um valor
-- próprio fixo. O valor próprio continua em `max_price`, então quem já tinha
-- teto fica com ele como "valor próprio", sem mexer em linha nenhuma. O método
-- fica em `ceiling_method`; os dois nunca coexistem.
--
-- Ações Nacionais aceitam Bazin, Graham e "Pelas duas" (`combined`); FIIs,
-- valor patrimonial (`nav`), Rendimento (`fund_yield`) e "Pelas duas". As duas
-- classes são sempre da B3 (o handler recusa tirar a marca), e a classe de um
-- ativo não muda, então a restrição pela classe basta.
alter table assets add column ceiling_method text
    check (ceiling_method in ('bazin', 'graham', 'combined', 'nav', 'fund_yield'));

alter table assets add constraint assets_ceiling_method_or_value
    check (ceiling_method is null or max_price is null);

alter table assets add constraint assets_ceiling_method_by_class
    check (
        ceiling_method is null
        or (asset_class = 'br_stocks' and ceiling_method in ('bazin', 'graham', 'combined'))
        or (asset_class = 'br_reits' and ceiling_method in ('nav', 'fund_yield', 'combined'))
    );

-- Os prêmios do rendimento-alvo dos FIIs, em pontos percentuais ao ano, ao
-- lado do rendimento-alvo do Bazin. Nulo = o padrão do código (tijolo 2,5;
-- papel 1,0). O rendimento-alvo do Bazin também passa a aceitar nulo (= 6%),
-- para quem só mexe num prêmio não ter o Bazin congelado no padrão de hoje.
alter table analysis_settings alter column bazin_yield drop not null;
alter table analysis_settings
    add column brick_premium numeric(4, 2) check (brick_premium between 0 and 20),
    add column paper_premium numeric(4, 2) check (paper_premium between 0 and 20);
