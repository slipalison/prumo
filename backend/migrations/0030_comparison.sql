-- Comparador de ativos (docs/comparador.md). O cache `price_history` guarda o
-- fechamento REAL do dia (o que se pagava); para medir o retorno de um ativo
-- que desdobrou, o comparador precisa dos desdobramentos e bonificações que o
-- Yahoo informa. Para ativo da carteira eles estão no livro; para ativo de fora
-- dela, só aqui.

-- Cada evento: a data do desdobramento e o fator (cotas novas por cota antiga,
-- `numerator / denominator`; 2 = 1 para 2, 0,1 = grupamento de 10 para 1).
create table price_splits (
    symbol text not null,
    ex_date date not null,
    factor numeric(24, 12) not null check (factor > 0),
    primary key (symbol, ex_date)
);

-- Quando e desde que data os desdobramentos de um símbolo foram conferidos na
-- fonte. Sem a marca, um símbolo sem desdobramento seria conferido a cada
-- pedido; com ela, no máximo uma vez por dia.
create table price_split_checks (
    symbol text primary key,
    checked_from date not null,
    checked_at timestamptz not null
);
