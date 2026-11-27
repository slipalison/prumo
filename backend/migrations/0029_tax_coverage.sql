-- IR, cobertura completa: a Selic do mês para os juros do DARF em atraso e o
-- IR retido na fonte que a pessoa informa no lugar da conta pela regra.
-- Regra, fontes e limites em docs/ir.md.

-- Selic acumulada no mês (% no mês), a série 4390 do SGS do Banco Central:
-- é a taxa que a Receita soma, mês a mês, nos juros de mora (Lei 9.430/1996,
-- art. 61, par. 3, e art. 5, par. 3). Referência compartilhada entre as
-- carteiras, como `index_history`, mas em tabela própria: a série é mensal,
-- precisa de história desde o primeiro ano com regra de IR (e não desde a
-- primeira foto da carteira) e só entra mês FECHADO (o SGS publica o mês
-- corrente acumulado até a véspera, que não é a taxa do mês).
create table tax_selic_monthly (
    -- Dia 1º do mês de referência.
    month date primary key check (extract(day from month) = 1),
    rate numeric(10, 4) not null check (rate >= 0 and rate < 100),
    fetched_at timestamptz not null
);

-- O IR retido na fonte do mês (o 0,005% das vendas e o 1% do day trade), como
-- a pessoa leu na nota de corretagem ou no informe da corretora. Sem linha, a
-- conta usa a retenção pela regra (domain::tax::monthly).
create table tax_withholding_informed (
    user_id uuid not null references users (id) on delete cascade,
    year integer not null check (year between 2000 and 2100),
    month smallint not null check (month between 1 and 12),
    amount numeric(20, 2) not null check (amount >= 0),
    updated_at timestamptz not null default now(),
    primary key (user_id, year, month)
);
