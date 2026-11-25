-- Alertas fora do app, pelo Telegram, em resumo diário (docs/alertas.md).

-- Uma linha por pessoa, criada no primeiro ajuste ou na primeira vinculação.
-- `telegram_chat_id` nulo = sem Telegram vinculado: nada é enviado.
create table alert_settings (
    user_id uuid primary key references users (id) on delete cascade,
    telegram_chat_id bigint,
    linked_at timestamptz,
    -- Horário de Brasília do resumo diário.
    send_at time not null default '08:00' check (date_part('second', send_at) = 0),
    -- Resumo sem valores em reais: só nomes e percentuais.
    hide_values boolean not null default false,
    -- Último dia (em Brasília) em que o resumo foi apurado: o resumo sai uma
    -- vez por dia, com ou sem nada a dizer.
    last_run_on date,
    updated_at timestamptz not null default now()
);

-- Um chat do Telegram pertence a uma pessoa só.
create unique index alert_settings_one_user_per_chat
    on alert_settings (telegram_chat_id) where telegram_chat_id is not null;

-- Cada tipo de alerta, ligado ou desligado, com o limite quando tem. Só existe
-- linha para o tipo que a pessoa mexeu; o resto vale o padrão do código.
create table alert_types (
    user_id uuid not null references users (id) on delete cascade,
    kind text not null check (
        kind in ('income_tomorrow', 'class_drift', 'price_below_ceiling', 'objective_achieved', 'tax_exemption', 'health_warning')
    ),
    enabled boolean not null,
    -- Pontos percentuais (`class_drift`) ou percentual do limite (`tax_exemption`).
    threshold numeric(10, 2) check (threshold > 0),
    -- O tipo "de transição" só avisa o que MUDA: a primeira apuração depois de
    -- ligar guarda o que já valia, sem avisar (senão ligar o tipo despejaria
    -- tudo o que já estava assim).
    seeded boolean not null default false,
    primary key (user_id, kind)
);

-- O que já valia na última apuração de cada tipo de transição (classe fora da
-- meta, ativo abaixo do teto, objetivo atingido, aviso de saúde): o resumo
-- só traz o que NÃO estava aqui.
create table alert_seen (
    user_id uuid not null references users (id) on delete cascade,
    kind text not null,
    key text not null check (char_length(key) between 1 and 200),
    primary key (user_id, kind, key)
);

-- Código de uso único para vincular o chat: a pessoa manda `/start <código>`
-- ao bot. Vale por poucos minutos.
create table alert_link_codes (
    code text primary key check (code ~ '^[A-Z0-9]{10}$'),
    user_id uuid not null references users (id) on delete cascade,
    expires_at timestamptz not null
);

create index alert_link_codes_by_user on alert_link_codes (user_id);

-- De onde o `getUpdates` continua (o `offset` do Telegram), compartilhado entre
-- as réplicas: quem assumir o polling continua de onde a outra parou.
create table telegram_poll_state (
    id smallint primary key check (id = 1),
    next_offset bigint not null
);
