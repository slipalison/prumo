-- Desempenho: o custo da compra (o que se pagou acima do fechamento do dia,
-- mais as taxas) passa a ser de todo o dinheiro que ficou na carteira depois
-- da compra, e não só das cotas de ontem (docs/desempenho.md, "Método de
-- cotas"). As séries guardadas foram feitas com a regra antiga: cada pessoa
-- com livro tem a série refeita desde o primeiro lançamento, em segundo plano,
-- logo depois do deploy. Nenhum lançamento muda.
select performance_mark_dirty(user_id, min(trade_date))
  from transactions
 group by user_id;
