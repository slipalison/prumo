-- Ativo negociado na B3 dentro de uma classe de fora dela: ETF de bitcoin em
-- Criptomoedas, BDR em Ações Internacionais, ETF de IMA-B em Renda Fixa.
--
-- A classe diz o que o ativo é para a carteira; esta coluna, onde ele é
-- cotado. Em Ações Nacionais e FIIs ela é sempre verdadeira — o mercado delas
-- é a B3 —, e a restrição impede o estado que não existe.

alter table assets add column listed_on_b3 boolean not null default false;

update assets set listed_on_b3 = true where asset_class in ('br_stocks', 'br_reits');

alter table assets add constraint assets_b3_classes_are_listed
    check (listed_on_b3 or asset_class not in ('br_stocks', 'br_reits'));
