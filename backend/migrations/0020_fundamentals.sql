-- Fundamentos: indicadores das empresas e dos FIIs da carteira, dos dados
-- abertos da CVM (dados.cvm.gov.br). Tudo aqui e REFERENCIA compartilhada
-- entre as carteiras, como `quotes`: o balanco da WEG e o mesmo para todo
-- mundo. As contas, as formulas e os limites estao em docs/fundamentos.md.

-- Emissor da B3 (as quatro letras do ticker) -> codigo CVM da companhia, da
-- pagina publica do emissor (`GetListedSupplementCompany`, campo `codeCVM`).
-- `cvm_code` nulo: a B3 nao informa (ETF cadastrado em Acoes Nacionais).
create table b3_issuer_cvm (
    issuer text primary key check (issuer ~ '^[A-Z0-9]{4}$'),
    cvm_code integer check (cvm_code > 0),
    fetched_at timestamptz,
    -- A vez de uma replica buscar (mesmo esquema de `b3_issuer_fetches`).
    claimed_until timestamptz
);

-- Cadastro das companhias abertas (`cad_cia_aberta.csv`), inteiro: ~2.600
-- linhas, para um ativo novo nao precisar baixar de novo.
create table cvm_companies (
    cvm_code integer primary key check (cvm_code > 0),
    cnpj text not null check (char_length(cnpj) <= 20),
    name text not null check (char_length(name) between 1 and 200),
    -- `SIT`: ATIVO, CANCELADA, SUSPENSO(A) - DECISAO ADM
    status text not null check (char_length(status) <= 60),
    -- `SIT_EMISSOR`: FASE OPERACIONAL, EM RECUPERACAO JUDICIAL OU EQUIVALENTE...
    issuer_status text check (char_length(issuer_status) <= 80),
    sector text check (char_length(sector) <= 120),
    updated_at timestamptz not null
);

-- Um documento entregue a CVM (DFP anual ou ITR trimestral) de uma companhia
-- da carteira, na versao mais alta que o arquivo trouxe. So as contas que a
-- conta usa (domain::fundamentals::keeps_line), nao o balanco inteiro.
create table cvm_filings (
    cvm_code integer not null check (cvm_code > 0),
    kind text not null check (kind in ('annual', 'quarterly')),
    reference_date date not null,
    -- `VERSAO`: reapresentacao. A maior vence; uma menor nunca troca a maior.
    version integer not null check (version >= 0),
    -- Consolidada quando a companhia entrega; senao, individual.
    scope text not null check (scope in ('consolidated', 'individual')),
    company_name text not null check (char_length(company_name) <= 200),
    -- O arquivo de onde veio (`annual:2025`), para conferencia.
    source text not null check (char_length(source) <= 40),
    fetched_at timestamptz not null,
    primary key (cvm_code, kind, reference_date)
);

create table cvm_filing_lines (
    cvm_code integer not null,
    kind text not null,
    reference_date date not null,
    statement text not null check (statement in ('bpa', 'bpp', 'dre', 'dfc', 'dva')),
    -- `ORDEM_EXERC`: ULTIMO (o exercicio entregue) ou PENULTIMO (a comparacao).
    period text not null check (period in ('current', 'previous')),
    period_start date,
    period_end date not null,
    code text not null check (code ~ '^[0-9]+(\.[0-9]{1,3})*$'),
    description text not null check (char_length(description) <= 300),
    -- Em reais (a escala MIL ja multiplicada), sem arredondar.
    value numeric not null,
    primary key (cvm_code, kind, reference_date, statement, period, code),
    foreign key (cvm_code, kind, reference_date)
        references cvm_filings (cvm_code, kind, reference_date) on delete cascade
);

-- Informe mensal de FII (`inf_mensal_fii_*.zip`): o mais novo de cada fundo,
-- de todos os fundos com ISIN de cota (~600 linhas).
create table cvm_funds (
    cnpj text primary key check (char_length(cnpj) <= 20),
    -- Raiz do ISIN da cota (BRKNRICTF007 -> KNRI): a chave do ticker.
    issuer text not null check (issuer ~ '^[A-Z0-9]{4}$'),
    isin text not null check (char_length(isin) = 12),
    name text not null check (char_length(name) <= 200),
    reference_date date not null,
    version integer not null check (version >= 0),
    net_assets numeric,
    shares numeric,
    updated_at timestamptz not null
);

create index cvm_funds_by_issuer on cvm_funds (issuer, reference_date desc);

-- Vacancia fisica do informe trimestral mais novo de cada fundo: a media dos
-- imoveis para renda acabados, ponderada pela area. Nula com o motivo em
-- `vacancy_gap` (fundo sem imovel para renda).
create table cvm_fund_vacancy (
    cnpj text primary key check (char_length(cnpj) <= 20),
    reference_date date not null,
    version integer not null check (version >= 0),
    vacancy numeric check (vacancy between 0 and 1),
    vacancy_gap text check (vacancy_gap in ('no_rental_property')),
    properties integer not null check (properties >= 0),
    updated_at timestamptz not null,
    check ((vacancy is null) <> (vacancy_gap is null))
);

-- Estado de cada arquivo da CVM: o validador da ultima versao lida (ETag e
-- Last-Modified, para o GET condicional responder 304 quando nada mudou), os
-- codigos CVM que essa versao ja teve lidos (um ativo novo de outra companhia
-- obriga a ler de novo) e a vez de uma replica.
create table cvm_sources (
    source text primary key check (source ~ '^[a-z_]+(:[0-9]{4})?$'),
    etag text check (char_length(etag) <= 200),
    last_modified text check (char_length(last_modified) <= 100),
    scanned_codes integer[] not null default '{}',
    attempted_at timestamptz,
    checked_at timestamptz,
    last_error text check (char_length(last_error) <= 300),
    claimed_until timestamptz
);
