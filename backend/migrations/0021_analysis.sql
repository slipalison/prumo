-- Analise da carteira: a Saude da carteira vira "Analise" e ganha regras de
-- dividendos e de rentabilidade, alem das de diversificacao.
--
-- As regras da pessoa continuam em `health_rules` (so existe linha para a
-- regra que ela mexeu; o resto vale o padrao do codigo). Nao ha dado para
-- migrar: as linhas que ja existem ficam exatamente como estao, e a tabela
-- apenas passa a aceitar as regras novas. Quem nunca mexeu nelas as ve ligadas,
-- com o limite de fabrica.
alter table health_rules drop constraint health_rules_rule_check;
alter table health_rules add constraint health_rules_rule_check check (
    rule in (
        -- diversificacao
        'max_asset', 'top_five', 'max_sector', 'min_assets', 'empty_class',
        'min_portfolio_assets', 'min_classes', 'min_sectors', 'min_abroad', 'min_low_risk',
        -- dividendos
        'min_yield', 'min_paying_share', 'income_growth', 'income_every_month', 'max_payer',
        'brick_reit',
        -- rentabilidade
        'beat_cdi_12m', 'beat_cdi_5y', 'beat_ibov_12m', 'beat_ibov_5y',
        'beat_ipca_12m', 'beat_ipca_5y', 'min_positive_years', 'max_volatility', 'max_loss'
    )
);

-- O que a Analise guarda da pessoa fora das regras: o rendimento-alvo da
-- referencia de Bazin (percentual ao ano). So existe linha para quem mexeu; o
-- padrao do codigo e 6%.
create table analysis_settings (
    user_id uuid primary key references users (id) on delete cascade,
    bazin_yield numeric(5, 2) not null check (bazin_yield > 0 and bazin_yield <= 100)
);
