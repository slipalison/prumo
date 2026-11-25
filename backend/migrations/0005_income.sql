-- Proventos (dividendo, JCP, rendimento de FII, amortização), objetivos da
-- pessoa e o cache do histórico de dividendos por cota.

-- Um pagamento que a carteira recebeu ou vai receber. `status` não é coluna:
-- recebido ou a receber se calcula pela `pay_date` contra o dia de hoje.
--
-- O ativo pode ser apagado depois; o evento guarda símbolo e nome, como os
-- itens do aporte, para o histórico de renda continuar de pé.
create table income_events (
    id uuid primary key,
    user_id uuid not null references users (id) on delete cascade,
    asset_id uuid references assets (id) on delete set null,
    symbol text,
    display_name text not null check (char_length(display_name) between 1 and 120),
    kind text not null check (kind in ('dividend', 'jcp', 'reit_income', 'amortization', 'other')),
    -- Data ex. Pode faltar num lançamento manual.
    ex_date date,
    pay_date date not null,
    amount_per_unit numeric(24, 8) not null check (amount_per_unit >= 0),
    quantity numeric(28, 10) not null check (quantity >= 0),
    gross numeric(20, 2) not null check (gross >= 0),
    -- IR retido na fonte (JCP: 15% do bruto).
    withholding numeric(20, 2) not null default 0 check (withholding >= 0),
    net numeric(20, 2) not null check (net >= 0),
    source text not null check (source in ('manual', 'estimate', 'b3_import')),
    -- Identificador no extrato de origem: a importação repetida não duplica.
    external_ref text,
    created_at timestamptz not null default now(),
    constraint income_events_net_is_gross_minus_withholding check (net = gross - withholding),
    constraint income_events_ex_before_pay check (ex_date is null or ex_date <= pay_date)
);

create index income_events_by_pay_date on income_events (user_id, pay_date desc);
create index income_events_by_asset on income_events (asset_id) where asset_id is not null;
create unique index income_events_one_external_ref
    on income_events (user_id, external_ref) where external_ref is not null;

-- Objetivos da pessoa. `kind` cobre os três do roteiro; a meta de renda mensal
-- (`monthly_income`) é o único que a tela Renda usa. O nome NÃO é `goals`: no
-- Prumo, `goals` são as metas por classe.
create table objectives (
    id uuid primary key,
    user_id uuid not null references users (id) on delete cascade,
    kind text not null check (kind in ('net_worth', 'monthly_income', 'class_value')),
    asset_class text check (
        asset_class in ('br_stocks', 'br_reits', 'intl_stocks', 'us_reits', 'crypto', 'fixed_income', 'intl_fixed_income')
    ),
    target numeric(20, 2) not null check (target > 0),
    -- Aporte mensal de premissa; nulo = a média real do livro.
    monthly_contribution numeric(20, 2) check (monthly_contribution >= 0),
    achieved_at timestamptz,
    created_at timestamptz not null default now()
);

create index objectives_by_user on objectives (user_id, kind, created_at);

-- Cache do histórico de dividendo por cota do provedor, compartilhado entre as
-- carteiras como `quotes`: o dividendo de KNRI11.SA é o mesmo para todos. É
-- insumo de ESTIMATIVA; o que a pessoa recebeu mora em `income_events`.
create table dividend_history (
    symbol text not null,
    ex_date date not null,
    amount numeric(24, 8) not null check (amount > 0),
    primary key (symbol, ex_date)
);

-- Quando cada símbolo foi buscado (um símbolo sem dividendo não tem linha em
-- `dividend_history`, mas tem esta) e em que moeda o provedor informa o valor.
create table dividend_fetches (
    symbol text primary key,
    currency text not null,
    fetched_at timestamptz not null
);
