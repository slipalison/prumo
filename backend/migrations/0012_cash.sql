-- Caixa: o dinheiro que a carteira gerou e ainda nao foi reaplicado.
--
-- Nao existe tabela `cash_balance`: o saldo e CALCULADO a cada leitura
-- (`domain/cash.rs`), a partir de tres fontes que ja sao a verdade de cada
-- coisa — o provento liquido recebido (`income_events`), o valor liquido das
-- vendas (o livro, `transactions`) e o caixa que cada aporte consumiu
-- (`contributions.cash_used`, esta migracao). Uma tabela de saldo seria uma
-- segunda fonte da verdade: editar ou apagar um provento ou uma venda a
-- deixaria errada, e o livro refaz tudo do zero justamente para isso nao
-- acontecer.

-- Quanto do aporte veio do caixa (o resto, `amount - cash_used`, e dinheiro de
-- fora). Nunca passa do valor do aporte nem do saldo na hora do registro; o
-- servidor confere, e a regra do valor vive aqui tambem.
alter table contributions
    add column cash_used numeric(20, 2) not null default 0 check (cash_used >= 0 and cash_used <= amount);

-- A data (Brasilia) a partir da qual o caixa conta. O que entrou antes dela
-- nao e caixa: ja foi gasto, ou nunca foi do Prumo. Sem isso, todo dividendo
-- ja recebido na vida da pessoa viraria "caixa" no primeiro dia.
--
-- Quem ja existe comeca no dia SEGUINTE ao do ultimo aporte (a mesma regra de
-- "proventos desde o ultimo aporte" que o Prumo mostrava: o pagamento do proprio
-- dia do aporte ja entrou na conta dele) ou, sem aporte, hoje: o caixa nasce
-- zerado e cresce dali. Quem chega depois comeca no dia em que chega.
alter table users
    add column cash_since date not null default ((now() at time zone 'America/Sao_Paulo')::date);

update users u
   set cash_since = coalesce(
       (select (max(c.created_at) at time zone 'America/Sao_Paulo')::date + 1
          from contributions c where c.user_id = u.id),
       (now() at time zone 'America/Sao_Paulo')::date);
