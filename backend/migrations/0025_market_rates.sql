-- Taxas de mercado e composição dos FIIs para o rendimento-alvo
-- (docs/analise.md § Referências de FII, docs/tesouro.md § Juro real).
--
-- O juro real de referência: a taxa de compra (`Taxa Compra Manha`) do título
-- IPCA+ do Tesouro Direto que vence mais perto de 10 anos depois da data-base
-- (a NTN-B vence a NTN-B Principal). A rodada diária do Tesouro escolhe e
-- grava; uma linha só, compartilhada entre as carteiras (como `quotes`).
create table treasury_real_rate (
    singleton boolean primary key default true check (singleton),
    -- A chave do título (`TD-IPCAJS-20370515`), que dá o nome que a tela mostra.
    bond_symbol text not null check (bond_symbol ~ '^TD-IPCA(JS)?-[0-9]{8}$'),
    -- % ao ano acima do IPCA.
    rate numeric(8, 4) not null,
    base_date date not null,
    updated_at timestamptz not null default now()
);

-- O que a carteira de cada FII tem, pelo balanço do informe mensal da CVM
-- (frações de imóveis, recebíveis e cotas de FII; somam 1). Pondera o alvo do
-- FII híbrido. Só a fonte da CVM tem; as linhas da B3 ficam nulas.
alter table sector_reference
    add column brick_share numeric(7, 6),
    add column paper_share numeric(7, 6),
    add column funds_share numeric(7, 6);

alter table sector_reference add constraint sector_reference_composition
    check (
        (brick_share is null and paper_share is null and funds_share is null)
        or (
            source = 'cvm_reits'
            and brick_share between 0 and 1
            and paper_share between 0 and 1
            and funds_share between 0 and 1
            and brick_share + paper_share + funds_share = 1
        )
    );

-- A referência dos FIIs que já existe fica sem composição (colunas nulas). Não
-- há marca a zerar aqui: o código novo relê a CVM enquanto houver linha sem
-- composição (`Store::sector_reference_incomplete`), o que vale também se um
-- pod antigo reler a fonte durante o deploy.
