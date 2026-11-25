-- Dois guarda-corpos do motor do aporte, os dois opcionais e escolhidos pela
-- própria pessoa (docs/alocacao.md § Teto de preço e limite de concentração).
--
-- Teto de preço por ativo, em reais por unidade: com o preço acima dele, o
-- ativo fica fora daquele aporte. Nulo = sem teto. Não existe em renda fixa
-- por valor (que não tem preço por unidade); o handler recusa.
alter table assets add column max_price numeric(24, 8) check (max_price > 0);

-- Configuração do motor por pessoa. Uma linha por pessoa, criada no primeiro
-- ajuste; sem linha, tudo desligado.
--
-- `max_asset_bp`: nenhum ativo passa desta fatia da carteira depois do aporte,
-- em pontos-base (1500 = 15%). Nulo = sem limite.
create table allocation_settings (
    user_id uuid primary key references users (id) on delete cascade,
    max_asset_bp integer check (max_asset_bp between 1 and 10000),
    updated_at timestamptz not null default now()
);
