-- Desempenho: as séries de referência (cache de fontes públicas) e a foto
-- diária de cada carteira pelo método de cotas. Método e limites em
-- docs/desempenho.md.

-- Índices de referência, compartilhados entre as carteiras (o CDI é o mesmo
-- para todos). CDI e Selic em % ao dia, IPCA em % no mês (datado no dia 1º),
-- Ibovespa em pontos.
create table index_history (
    code text not null check (code in ('cdi', 'selic', 'ipca', 'ibov')),
    date date not null,
    value numeric(24, 10) not null,
    primary key (code, date)
);

-- Fechamento diário dos símbolos do Yahoo que alguma carteira usa
-- (`KNRI11.SA`, `AAPL`, `BTC-USD`, `USDBRL=X`), na moeda do símbolo. É o
-- `close` (não o ajustado por proventos): o preço que se pagava naquele dia.
create table price_history (
    symbol text not null,
    date date not null,
    close numeric(24, 8) not null check (close > 0),
    primary key (symbol, date)
);

-- Uma foto por pessoa e por dia (no horário de Brasília). O `upsert` por
-- (user_id, date) faz a tarefa diária ser idempotente: rodar de novo no mesmo
-- dia refaz a foto, não cria outra.
create table portfolio_snapshots (
    user_id uuid not null references users (id) on delete cascade,
    date date not null,
    -- Patrimônio no fechamento, em reais.
    market_value numeric(20, 2) not null check (market_value >= 0),
    -- O que entrou (+) ou saiu (−) de dinheiro desde a foto anterior. Na
    -- primeira foto, o patrimônio inteiro (é o "aplicado" inicial).
    net_flow numeric(20, 2) not null,
    quota_value numeric(30, 12) not null check (quota_value > 0),
    quotas numeric(30, 12) not null check (quotas >= 0),
    -- Ativo, quantidade e preço em reais de cada posição: a foto seguinte
    -- compara com esta para separar fluxo de rendimento.
    positions jsonb not null default '[]',
    -- Quando a foto foi tirada: os aportes registrados depois entram na
    -- foto seguinte.
    taken_at timestamptz not null,
    primary key (user_id, date)
);
