-- Agenda (docs/agenda.md): os documentos entregues pelas companhias e pelos
-- FIIs da carteira e a data de entrega de cada ITR e DFP. Tudo aqui e
-- REFERENCIA compartilhada entre as carteiras, como os fundamentos: o fato
-- relevante de uma companhia e o mesmo para todo mundo. So entra o que e de
-- companhia ou fundo que alguem tem na carteira.

-- Um documento entregue: fato relevante, comunicado ao mercado, aviso aos
-- acionistas ou cotistas, assembleia, resultado (FII) e relatorio gerencial.
-- Companhia pelo IPE da CVM (dados abertos, `ipe_cia_aberta_AAAA.zip`); FII
-- pelo Fundos.NET (a CVM nao publica os eventuais de FII nos dados abertos).
create table corporate_documents (
    source text not null check (source in ('cvm_ipe', 'fundos_net')),
    -- `numSequencia` do link da CVM; `id` do Fundos.NET.
    document_id text not null check (document_id ~ '^[0-9]{1,20}$'),
    cvm_code integer check (cvm_code > 0),
    -- Como o informe mensal da CVM escreve (`cvm_funds.cnpj`).
    fund_cnpj text check (char_length(fund_cnpj) <= 20),
    category text not null check (category in (
        'material_fact', 'market_notice', 'shareholder_notice', 'meeting', 'results',
        'management_report'
    )),
    -- Tipo e especie como a fonte escreve ("AGO - Edital de Convocacao").
    title text check (char_length(title) <= 200),
    subject text check (char_length(subject) <= 400),
    reference_date date,
    delivered_on date not null,
    version integer not null check (version >= 0),
    -- O documento original, so no RAD da CVM ou no Fundos.NET.
    url text not null check (char_length(url) <= 300 and url ~ '^https://'),
    updated_at timestamptz not null,
    primary key (source, document_id),
    check ((source = 'cvm_ipe') = (cvm_code is not null)),
    check ((source = 'fundos_net') = (fund_cnpj is not null))
);

create index corporate_documents_by_company
    on corporate_documents (cvm_code, delivered_on) where cvm_code is not null;
create index corporate_documents_by_fund
    on corporate_documents (fund_cnpj, delivered_on) where fund_cnpj is not null;

-- A entrega de cada ITR e DFP das companhias da carteira, do indice dos
-- arquivos de dados abertos (`DT_RECEB`, `LINK_DOC`): a data em que o
-- resultado chegou a CVM. A versao maior vence (reapresentacao).
create table cvm_filing_receipts (
    cvm_code integer not null check (cvm_code > 0),
    kind text not null check (kind in ('annual', 'quarterly')),
    reference_date date not null,
    version integer not null check (version >= 0),
    received_on date not null,
    url text check (char_length(url) <= 300 and url ~ '^https://'),
    updated_at timestamptz not null,
    primary key (cvm_code, kind, reference_date)
);

create index cvm_filing_receipts_by_date on cvm_filing_receipts (cvm_code, received_on);

-- A vez de cada replica buscar um fundo no Fundos.NET (o mesmo esquema de
-- `b3_issuer_fetches`) e a ultima falha, para esperar antes de tentar de novo.
-- O estado dos arquivos da CVM da Agenda fica em `cvm_sources`, com as chaves
-- `documents:AAAA`, `receipts_annual:AAAA` e `receipts_quarterly:AAAA`.
create table fund_document_fetches (
    cnpj text primary key check (char_length(cnpj) <= 20),
    fetched_at timestamptz,
    attempted_at timestamptz,
    last_error text check (char_length(last_error) <= 300),
    claimed_until timestamptz
);
