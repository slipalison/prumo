-- Título do Tesouro Direto cadastrado pelo nome ("Tesouro IPCA+ 2035") e cotado
-- sozinho pelo preço de resgate do dia.
--
-- A classe continua Renda Fixa: o que o título é para a carteira não muda. A
-- marca diz de onde vem o preço — não é B3 (`listed_on_b3`) nem valor
-- informado (sem símbolo), é a tabela do Tesouro Transparente, que o Prumo
-- baixa 1x por dia e grava em `quotes` com o símbolo `TD-<tipo>-<vencimento>`.
--
-- Só Renda Fixa pode ter a marca, e com ela o ativo tem símbolo e não é da B3:
-- a restrição impede os estados que não existem.

alter table assets add column treasury boolean not null default false;

alter table assets add constraint assets_treasury_is_fixed_income
    check (not treasury or (asset_class = 'fixed_income' and symbol is not null and not listed_on_b3));
