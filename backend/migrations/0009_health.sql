-- Saude da carteira: setor/segmento de cada papel e as regras de aviso de cada
-- pessoa.

-- Referencia de setor, compartilhada entre as carteiras (como `quotes`): o
-- setor do WEGE3 e o mesmo para todo mundo. A chave e o codigo de quatro
-- letras do emissor (WEGE3 -> WEGE, KNRI11 -> KNRI), que e o que as duas fontes
-- publicas trazem. `source` diz de onde veio e escolhe a chave certa para cada
-- classe: Acoes Nacionais -> B3, Fundos Imobiliarios -> CVM.
create table sector_reference (
    source text not null check (source in ('b3_stocks', 'cvm_reits')),
    code text not null check (code ~ '^[A-Z0-9]{3,6}$'),
    -- Acoes: setor economico da B3. FIIs: tipo (tijolo, papel, fundo de
    -- fundos ou hibrido), que o Prumo deriva da carteira do fundo.
    sector text not null check (char_length(sector) between 1 and 120),
    -- So a B3 tem subsetor.
    subsector text check (char_length(subsector) between 1 and 120),
    -- Acoes: segmento da B3. FIIs: segmento de atuacao declarado a CVM.
    segment text check (char_length(segment) between 1 and 120),
    updated_at timestamptz not null default now(),
    primary key (source, code)
);

-- Quando cada fonte foi buscada: o servico so busca de novo passado o prazo, e
-- espera um tempo depois de uma falha em vez de bater na fonte a cada rodada.
create table sector_sync (
    source text primary key check (source in ('b3_stocks', 'cvm_reits')),
    last_attempt_at timestamptz not null,
    last_success_at timestamptz,
    entries integer not null default 0 check (entries >= 0),
    last_error text
);

-- Regra ligada/desligada e o limite de cada uma. So existe linha para a regra
-- que a pessoa mexeu; o resto vale o padrao do codigo.
create table health_rules (
    user_id uuid not null references users (id) on delete cascade,
    rule text not null check (rule in ('max_asset', 'top_five', 'max_sector', 'min_assets', 'empty_class')),
    enabled boolean not null,
    -- Percentual ou quantidade, conforme a regra; nulo em `empty_class`.
    threshold numeric(10, 2) check (threshold > 0),
    primary key (user_id, rule)
);
