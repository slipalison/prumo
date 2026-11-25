-- Esquema inicial do Prumo.
--
-- Cada carteira pertence a um e-mail (a identidade que o Cloudflare Access
-- entrega). Dinheiro e quantidade em `numeric`, nunca em ponto flutuante.

create table users (
    id uuid primary key,
    email text not null unique,
    created_at timestamptz not null default now()
);

create table goals (
    user_id uuid not null references users (id) on delete cascade,
    asset_class text not null check (
        asset_class in ('br_stocks', 'br_reits', 'intl_stocks', 'us_reits', 'crypto', 'fixed_income', 'intl_fixed_income')
    ),
    target_bp integer not null check (target_bp between 0 and 10000),
    primary key (user_id, asset_class)
);

create table checklist_questions (
    id uuid primary key,
    user_id uuid not null references users (id) on delete cascade,
    checklist text not null check (checklist in ('stocks', 'real_estate')),
    position integer not null,
    prompt text not null check (char_length(prompt) between 1 and 300),
    created_at timestamptz not null default now()
);

create index checklist_questions_by_user on checklist_questions (user_id, checklist, position);

create table assets (
    id uuid primary key,
    user_id uuid not null references users (id) on delete cascade,
    asset_class text not null check (
        asset_class in ('br_stocks', 'br_reits', 'intl_stocks', 'us_reits', 'crypto', 'fixed_income', 'intl_fixed_income')
    ),
    -- Nulo nas classes por valor (renda fixa), que não têm cotação.
    symbol text,
    display_name text not null check (char_length(display_name) between 1 and 120),
    quantity numeric(28, 10) not null check (quantity >= 0),
    manual_price numeric(24, 8) check (manual_price > 0),
    score_override smallint check (score_override between 0 and 10),
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);

create index assets_by_user on assets (user_id);
create unique index assets_one_symbol_per_class on assets (user_id, asset_class, symbol) where symbol is not null;

create table asset_answers (
    asset_id uuid not null references assets (id) on delete cascade,
    question_id uuid not null references checklist_questions (id) on delete cascade,
    answer boolean not null,
    primary key (asset_id, question_id)
);

create index asset_answers_by_question on asset_answers (question_id);

-- Compartilhada entre as carteiras: a cotação de KNRI11.SA é a mesma para todos.
create table quotes (
    symbol text primary key,
    price numeric(24, 8) not null check (price > 0),
    currency text not null,
    name text,
    fetched_at timestamptz not null
);

create table contributions (
    id uuid primary key,
    user_id uuid not null references users (id) on delete cascade,
    amount numeric(20, 2) not null check (amount >= 0),
    created_at timestamptz not null default now()
);

create index contributions_by_user on contributions (user_id, created_at desc);

create table contribution_items (
    id uuid primary key,
    contribution_id uuid not null references contributions (id) on delete cascade,
    -- O ativo pode ser removido depois; o histórico guarda o nome e o símbolo.
    asset_id uuid references assets (id) on delete set null,
    symbol text,
    display_name text not null,
    quantity numeric(28, 10) not null check (quantity > 0),
    unit_price numeric(24, 8) not null check (unit_price > 0),
    value numeric(20, 2) not null check (value > 0)
);

create index contribution_items_by_contribution on contribution_items (contribution_id);
