-- Desempenho: o IFIX (índice dos fundos imobiliários da B3, em pontos) entra
-- como índice de referência, ao lado de CDI, Selic, IPCA e Ibovespa. Fonte e
-- método em docs/desempenho.md. Só alarga a lista de códigos aceitos: nenhum
-- dado existente muda.
alter table index_history drop constraint index_history_code_check;
alter table index_history add constraint index_history_code_check
    check (code in ('cdi', 'selic', 'ipca', 'ibov', 'ifix'));
