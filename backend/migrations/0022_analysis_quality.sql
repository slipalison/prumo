-- Analise da carteira: a dimensao Qualidade dos ativos passa a ter regras
-- (fundamentos da CVM, migracao 0020). A configuracao da pessoa continua em
-- `health_rules`: as linhas que ja existem ficam como estao, e a tabela apenas
-- passa a aceitar as oito regras novas. Quem nunca mexeu nelas as ve ligadas,
-- com o limite de fabrica (70% do valor).
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
        'beat_ipca_12m', 'beat_ipca_5y', 'min_positive_years', 'max_volatility', 'max_loss',
        -- qualidade dos ativos
        'quality_roe', 'quality_profit', 'quality_leverage', 'quality_margin',
        'quality_cash', 'quality_recovery', 'quality_fii_pvp', 'quality_fii_vacancy'
    )
);
