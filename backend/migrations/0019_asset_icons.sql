-- Logos dos ativos da B3 (docs/icones.md): um por símbolo, buscado na brapi uma
-- vez e guardado. O logo é público e igual para todas as pessoas, então a chave
-- é o símbolo, não a carteira.
--
-- `outcome` é o resultado da última tentativa:
--   pending: reservado por uma réplica que ainda busca (ou que caiu no meio);
--   found:   há logo (bytes e content_type preenchidos);
--   missing: a brapi não tem logo (ou o arquivo foi recusado), sem erro de rede;
--   failed:  rede, limite ou token — tenta de novo logo; os bytes de uma busca
--            anterior ficam (um logo velho vale mais que nenhum).
create table asset_icons (
    symbol text primary key check (symbol ~ '^[A-Z0-9]{1,12}$'),
    outcome text not null check (outcome in ('pending', 'found', 'missing', 'failed')),
    content_type text,
    -- O SVG já saneado (domain/icon.rs), nunca o que a fonte mandou.
    bytes bytea check (bytes is null or octet_length(bytes) <= 131072),
    -- Validador HTTP e versão do logo na URL.
    etag text generated always as (md5(bytes)) stored,
    source text not null default 'brapi',
    source_url text,
    -- Quando os bytes atuais foram buscados.
    fetched_at timestamptz,
    -- Quando a última tentativa terminou (qualquer resultado).
    attempted_at timestamptz,
    -- Desde quando uma réplica busca este símbolo. Quem encontra a reserva
    -- recente pula; a reserva velha (processo que caiu) vale como livre.
    claimed_at timestamptz,
    error text check (error is null or char_length(error) <= 300),
    check ((bytes is null) = (content_type is null)),
    check (outcome <> 'found' or bytes is not null)
);
