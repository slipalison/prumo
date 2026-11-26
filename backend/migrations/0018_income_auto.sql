-- Proventos automaticos: o anuncio da B3 (dividendo, JCP, rendimento) vezes a
-- quantidade do livro na data com vira provento, sem a pessoa digitar. Regra
-- inteira em docs/proventos-automaticos.md.

-- O provento automatico e uma origem nova. Ele e DERIVADO: a rodada refaz o
-- conjunto inteiro da pessoa a partir dos anuncios e do livro, e nunca toca o
-- que veio da pessoa (`manual`) ou do extrato (`b3_import`).
alter table income_events drop constraint income_events_source_check;
alter table income_events add constraint income_events_source_check
    check (source in ('manual', 'estimate', 'b3_import', 'b3_announcement'));

-- Anuncios de provento em dinheiro, por ISIN, como a B3 publica na pagina de
-- "Eventos corporativos" de cada emissor. Compartilhado entre as carteiras,
-- como `quotes`: o JCP de ITSA4 e o mesmo para todo mundo.
--
-- A chave inclui o valor: a mesma emissora anuncia dois JCP com a mesma data
-- com e valores diferentes (um por trimestre de referencia).
create table b3_cash_announcements (
    -- O codigo de negociacao sem o numero (ITSA, KNRI): o escopo da busca.
    issuer text not null check (issuer ~ '^[A-Z0-9]{4}$'),
    isin text not null check (isin ~ '^[A-Z]{2}[A-Z0-9]{9}[0-9]$'),
    -- O rotulo da B3 ("DIVIDENDO", "JRS CAP PROPRIO", "RENDIMENTO"...).
    label text not null check (char_length(label) between 1 and 40),
    -- A data com (`lastDatePrior`): o ultimo dia em que ter a cota dava direito.
    record_date date not null,
    -- Valor por cota, bruto, como a B3 publica (ate 11 casas).
    rate numeric(24, 11) not null check (rate > 0),
    -- Data de pagamento; nula enquanto a empresa nao define ("a definir").
    pay_date date,
    approved_on date,
    related_to text check (related_to is null or char_length(related_to) <= 80),
    primary key (isin, label, record_date, rate)
);

create index b3_cash_announcements_by_issuer on b3_cash_announcements (issuer, record_date);

-- Eventos em acoes (bonificacao, desdobramento, grupamento). NAO vao para o
-- livro (falta o custo atribuido): viram uma pendencia na tela.
create table b3_stock_announcements (
    issuer text not null check (issuer ~ '^[A-Z0-9]{4}$'),
    isin text not null check (isin ~ '^[A-Z]{2}[A-Z0-9]{9}[0-9]$'),
    label text not null check (char_length(label) between 1 and 40),
    record_date date not null,
    -- Como a B3 publica: percentual na bonificacao e no desdobramento (2 = 2%,
    -- 100 = cada acao vira duas) e multiplicador no grupamento (0,1 = dez viram uma).
    factor numeric(24, 11) not null check (factor > 0),
    approved_on date,
    primary key (isin, label, record_date, factor)
);

create index b3_stock_announcements_by_issuer on b3_stock_announcements (issuer, record_date);

-- O ISIN de cada codigo de negociacao, da propria B3 (`otherCodes` do
-- cadastro do emissor): ITSA3 -> BRITSAACNOR0, ITSA4 -> BRITSAACNPR7.
create table b3_listings (
    ticker text primary key check (ticker ~ '^[A-Z0-9]{4}[0-9]{1,2}$'),
    issuer text not null check (issuer ~ '^[A-Z0-9]{4}$'),
    isin text not null check (isin ~ '^[A-Z]{2}[A-Z0-9]{9}[0-9]$'),
    fetched_at timestamptz not null
);

-- Quando cada emissor foi buscado. `claimed_until` e a vez de uma replica:
-- quem marca primeiro busca, a outra pula (um `update` condicional, sem
-- segurar conexao durante a rede).
create table b3_issuer_fetches (
    issuer text primary key check (issuer ~ '^[A-Z0-9]{4}$'),
    fetched_at timestamptz,
    claimed_until timestamptz
);

-- Liga/desliga por pessoa. Ligado por padrao: quem ja existe ganha os
-- automaticos na primeira rodada depois do deploy.
alter table users add column income_auto boolean not null default true;

-- O automatico que a pessoa apagou nao volta na proxima rodada.
create table income_auto_dismissed (
    user_id uuid not null references users (id) on delete cascade,
    external_ref text not null check (char_length(external_ref) between 1 and 200),
    dismissed_at timestamptz not null default now(),
    primary key (user_id, external_ref)
);
