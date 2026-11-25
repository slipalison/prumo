-- Saude da carteira: as duas regras de renda (`income_every_month` e
-- `max_payer`) entram no que `health_rules` aceita. Nao ha dado para migrar:
-- a tabela guarda so o que a pessoa mexeu, e quem nunca mexeu nas regras novas
-- continua com o padrao do codigo (ligadas, 12 meses e 25%). As linhas que ja
-- existem ficam exatamente como estao.
alter table health_rules drop constraint health_rules_rule_check;
alter table health_rules add constraint health_rules_rule_check check (
    rule in (
        'max_asset', 'top_five', 'max_sector', 'min_assets', 'empty_class',
        'income_every_month', 'max_payer'
    )
);
