-- Objetivos com prazo: patrimônio total e valor numa classe (a tabela nasceu
-- na 0005, onde só `monthly_income` estava em uso).
--
-- O nome NÃO é `goals`: no Prumo, `goals` são as metas por classe.
--
-- `expected_return` é a premissa de rentabilidade, em % ao ano, que a pessoa
-- escolheu para a projeção deste objetivo. Nulo = o Prumo usa a rentabilidade
-- medida na tela Desempenho ou, sem série, o padrão documentado em
-- `docs/api.md`. O aporte mensal de premissa já existe (`monthly_contribution`,
-- nulo = a média real do livro).
alter table objectives
    add column expected_return numeric(7, 4)
        check (expected_return >= -50 and expected_return <= 100);

-- A classe só existe, e é obrigatória, no objetivo por classe. Nenhuma linha
-- de antes a viola: a única criada até aqui é `monthly_income`, sem classe.
alter table objectives
    add constraint objectives_class_only_for_class_value
        check ((kind = 'class_value') = (asset_class is not null));
