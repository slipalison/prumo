-- Risco: volatilidade, queda maxima, correlacao e contribuicao para o risco.
-- Metodo e limites em docs/risco.md.
--
-- Nenhuma tabela nova: o risco le `price_history` (o fechamento real), os
-- desdobramentos de `price_splits` (migracao 0030, a mesma do comparador, que
-- agora a sincronia de precos tambem enche), os proventos por cota de
-- `dividend_history`, o Ibovespa de `index_history` e a cota de
-- `portfolio_snapshots`.
--
-- Quem ja tem serie ganha, logo depois do deploy, o historico longo de preco
-- (a janela de 5 anos do risco) e a lista de desdobramentos: a marca de "sujo"
-- desde a ultima foto faz a reconstrucao em segundo plano rodar a sincronia de
-- precos na hora, sem esperar a tarefa das 19h. A marca vale so desde a ultima
-- foto; o preco antigo que chegar marca, como sempre, a serie de quem tem o
-- ativo desde o primeiro dia novo (uma reconstrucao, uma vez).
insert into performance_rebuilds (user_id, dirty_since, marked_at)
select user_id, max(date), now()
  from portfolio_snapshots
 group by user_id
on conflict (user_id) do update set
    dirty_since = least(performance_rebuilds.dirty_since, excluded.dirty_since),
    marked_at = excluded.marked_at;
