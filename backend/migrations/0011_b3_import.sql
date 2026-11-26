-- Importação do extrato da B3: o provento que veio de "Proventos a receber"
-- (provisionado). Quando o mesmo provento chega como recebido — às vezes com
-- outra data ou outro valor líquido —, a importação troca esta linha pela
-- recebida em vez de somar as duas (regra em docs/importacao-b3.md).
--
-- Só a importação marca: provento manual e estimativa nunca são provisórios.
alter table income_events
    add column provisional boolean not null default false,
    add constraint income_events_provisional_is_imported check (not provisional or source = 'b3_import');
