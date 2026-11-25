-- Série retroativa a partir do livro: a foto de cada dia desde o primeiro
-- lançamento, refeita quando o livro muda no passado. Método em
-- docs/desempenho.md.

-- Provento líquido recebido no dia (data de pagamento). É retorno, não aporte:
-- entra na conta da cota e não no "aplicado" (`net_flow`).
alter table portfolio_snapshots
    add column income numeric(20, 2) not null default 0 check (income >= 0);

-- A marca de "sujo desde a data X": a série da pessoa precisa ser refeita a
-- partir de `dirty_since`. Os gatilhos abaixo marcam; a tarefa em segundo plano
-- (`performance.rs`) refaz e apaga a marca se ela não mudou no meio do caminho
-- (`marked_at` igual ao que leu).
create table performance_rebuilds (
    user_id uuid primary key references users (id) on delete cascade,
    dirty_since date not null,
    marked_at timestamptz not null
);

-- Até onde, para trás, o cache de preços de um símbolo já foi pedido à fonte.
-- Sem esta marca, um símbolo que não tem dado antes de certa data (lançado
-- depois, ou fonte que não guarda tanto) seria pedido de novo a cada rodada.
create table price_coverage (
    symbol text primary key,
    covered_from date not null
);

-- Marca a série da pessoa como suja desde `since` (a menor data vence). Na
-- remoção da pessoa em cascata os lançamentos somem depois dela: aí não há o
-- que marcar, e o `exists` evita a chave estrangeira quebrada.
create function performance_mark_dirty(p_user uuid, p_since date) returns void
language sql as $$
    insert into performance_rebuilds (user_id, dirty_since, marked_at)
    select p_user, p_since, clock_timestamp()
     where exists (select 1 from users where id = p_user)
    on conflict (user_id) do update set
        dirty_since = least(performance_rebuilds.dirty_since, excluded.dirty_since),
        marked_at = excluded.marked_at
$$;

-- Lançamento gravado, editado (inclusive a data, ou o ativo removido, que solta
-- o `asset_id`) ou apagado: suja desde a menor data envolvida. Vale para toda
-- escrita no livro — a tela, o "Já comprei", a importação da B3 —, sem cada
-- caminho precisar lembrar.
create function transactions_mark_dirty() returns trigger
language plpgsql as $$
begin
    if tg_op in ('UPDATE', 'DELETE') then
        perform performance_mark_dirty(old.user_id, old.trade_date);
    end if;
    if tg_op in ('INSERT', 'UPDATE') then
        perform performance_mark_dirty(new.user_id, new.trade_date);
    end if;
    return null;
end
$$;

create trigger transactions_mark_dirty
    after insert or update or delete on transactions
    for each row execute function transactions_mark_dirty();

-- Provento: a data que conta é a de pagamento.
create function income_events_mark_dirty() returns trigger
language plpgsql as $$
begin
    if tg_op in ('UPDATE', 'DELETE') then
        perform performance_mark_dirty(old.user_id, old.pay_date);
    end if;
    if tg_op in ('INSERT', 'UPDATE') then
        perform performance_mark_dirty(new.user_id, new.pay_date);
    end if;
    return null;
end
$$;

create trigger income_events_mark_dirty
    after insert or update or delete on income_events
    for each row execute function income_events_mark_dirty();

-- O ativo que muda de símbolo, de classe, de fonte de preço ou de preço manual
-- muda o valor de todos os dias dele: suja desde o primeiro lançamento. A
-- quantidade e o custo (cache do livro) não entram: quem muda a quantidade é o
-- livro, que já marcou.
create function assets_mark_dirty() returns trigger
language plpgsql as $$
declare
    first_trade date;
begin
    select min(trade_date) into first_trade from transactions where asset_id = new.id;
    if first_trade is not null then
        perform performance_mark_dirty(new.user_id, first_trade);
    end if;
    return null;
end
$$;

create trigger assets_mark_dirty
    after update of symbol, asset_class, listed_on_b3, treasury, manual_price on assets
    for each row
    when (old.symbol is distinct from new.symbol
          or old.asset_class is distinct from new.asset_class
          or old.listed_on_b3 is distinct from new.listed_on_b3
          or old.treasury is distinct from new.treasury
          or old.manual_price is distinct from new.manual_price)
    execute function assets_mark_dirty();

-- Quem já tem livro ganha a série desde o primeiro lançamento no próximo
-- deploy: a marca vale desde a data mais antiga do livro de cada pessoa.
insert into performance_rebuilds (user_id, dirty_since, marked_at)
select user_id, min(trade_date), now()
  from transactions
 group by user_id;
