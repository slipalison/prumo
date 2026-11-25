-- Livro de lançamentos: toda mudança de quantidade vira um lançamento datado,
-- e a quantidade do ativo passa a ser a soma do livro.
--
-- `assets.quantity`, `assets.cost_basis` e `assets.realized_gain` viram cache
-- da posição, refeito do livro na mesma transação que grava, edita ou apaga um
-- lançamento (`store/ledger.rs`). A carteira ao vivo lê o cache e não refaz o
-- livro a cada preço que chega.

create table transactions (
    id uuid primary key,
    -- Ordem de entrada no livro: desempata lançamentos do mesmo dia.
    seq bigint generated always as identity,
    user_id uuid not null references users (id) on delete cascade,
    -- O ativo pode ser removido depois; o lançamento fica como histórico, com
    -- a foto do símbolo, da classe e do nome no momento.
    asset_id uuid references assets (id) on delete set null,
    symbol text,
    asset_class text not null check (
        asset_class in ('br_stocks', 'br_reits', 'intl_stocks', 'us_reits', 'crypto', 'fixed_income', 'intl_fixed_income')
    ),
    display_name text not null,
    kind text not null check (
        kind in ('opening', 'buy', 'sell', 'split', 'reverse_split', 'bonus', 'amortization', 'redemption', 'adjustment')
    ),
    trade_date date not null,
    -- Só o ajuste tem sinal. Em renda fixa por valor a quantidade é o saldo em reais.
    quantity numeric(28, 10) not null check (quantity <> 0 and (kind = 'adjustment' or quantity > 0)),
    -- Na moeda do lançamento. Nulo onde o tipo não tem preço, e no saldo
    -- inicial de custo desconhecido.
    unit_price numeric(24, 8) check (unit_price >= 0),
    fees numeric(20, 2) not null default 0 check (fees >= 0),
    currency text not null default 'BRL' check (currency ~ '^[A-Z]{3}$'),
    -- Reais por unidade da moeda. Nulo em reais; obrigatório em outra moeda
    -- quando há preço.
    fx_rate numeric(20, 10) check (fx_rate > 0),
    -- `migration`: criado por esta migração a partir dos dados de antes do livro.
    source text not null default 'manual' check (source in ('manual', 'plan', 'b3_import', 'migration')),
    -- Identificador da linha de origem (importação da B3), para reimportar sem duplicar.
    external_ref text,
    contribution_id uuid references contributions (id) on delete set null,
    created_at timestamptz not null default now(),
    check (kind not in ('buy', 'sell', 'redemption', 'amortization') or unit_price > 0),
    check (kind not in ('split', 'reverse_split', 'adjustment') or unit_price is null),
    check (fees = 0 or kind in ('opening', 'buy', 'sell', 'redemption')),
    check (currency <> 'BRL' or fx_rate is null),
    check (currency = 'BRL' or unit_price is null or fx_rate is not null)
);

create index transactions_by_asset on transactions (asset_id, trade_date, seq);
create index transactions_by_user on transactions (user_id, trade_date desc, seq desc);
create unique index transactions_one_external_ref on transactions (user_id, external_ref)
    where external_ref is not null;

-- Custo total e lucro realizado, em reais. Nulo = desconhecido (saldo inicial
-- sem preço): diferente de zero.
alter table assets
    add column cost_basis numeric default 0,
    add column realized_gain numeric default 0;

-- ------------------------------------------------------- dados de antes --
--
-- Antes do livro, a quantidade vinha de dois lugares: o que a pessoa digitou
-- (cadastro e edição) e o "Já comprei" (`contribution_items`). A migração
-- separa os dois:
--
-- 1. Cada item de aporte vira uma compra (`buy`, `source = 'plan'`) ligada ao
--    aporte, na data do aporte (horário de Brasília) e com o preço do item. Em
--    renda fixa por valor a compra é o valor do item com preço 1. Item de
--    ativo já removido não vira lançamento: não há posição para alimentar, e o
--    histórico dele continua em `contribution_items`.
-- 2. O que a pessoa digitou é a diferença `d = quantidade atual − Σ compras`:
--    - `d > 0`: saldo inicial (`opening`) com `d` e custo desconhecido, datado
--      no cadastro do ativo (ou no primeiro aporte, se for antes) e gravado
--      antes das compras, para vir primeiro no livro;
--    - `d = 0`: tudo veio dos aportes; nenhum saldo inicial, e o custo é o
--      das compras;
--    - `d < 0`: a pessoa baixou a quantidade à mão depois de comprar (vendeu
--      ou corrigiu). Vira um ajuste (`adjustment`) de `d`, datado no dia da
--      migração, depois das compras. O ajuste não mexe no custo: o custo fica
--      o das compras (zero se a posição zerou), e a pessoa pode trocar o
--      ajuste por uma venda com preço na folha do ativo.
-- 3. O cache de custo sai das mesmas regras, e no fim a migração confere que
--    a soma do livro é a quantidade de todo ativo — se não for, ela falha e
--    nada muda.

create temporary table ledger_bought on commit drop as
select ci.asset_id,
       sum(case when a.asset_class in ('fixed_income', 'intl_fixed_income') and not a.listed_on_b3
                then ci.value else ci.quantity end) as quantity,
       sum(case when a.asset_class in ('fixed_income', 'intl_fixed_income') and not a.listed_on_b3
                then ci.value else ci.quantity * ci.unit_price end) as cost,
       min((c.created_at at time zone 'America/Sao_Paulo')::date) as first_date
  from contribution_items ci
  join contributions c on c.id = ci.contribution_id
  join assets a on a.id = ci.asset_id
 group by ci.asset_id;

-- 2, d > 0: saldo inicial primeiro, para ter `seq` menor que as compras.
insert into transactions
    (id, user_id, asset_id, symbol, asset_class, display_name, kind, trade_date, quantity, unit_price, source, created_at)
select gen_random_uuid(), a.user_id, a.id, a.symbol, a.asset_class, a.display_name, 'opening',
       least((a.created_at at time zone 'America/Sao_Paulo')::date, coalesce(b.first_date, 'infinity'::date)),
       a.quantity - coalesce(b.quantity, 0), null, 'migration', a.created_at
  from assets a
  left join ledger_bought b on b.asset_id = a.id
 where a.quantity - coalesce(b.quantity, 0) > 0
 order by a.created_at, a.id;

-- 1: as compras dos aportes, na ordem em que foram registradas.
insert into transactions
    (id, user_id, asset_id, symbol, asset_class, display_name, kind, trade_date, quantity, unit_price,
     source, contribution_id, created_at)
select gen_random_uuid(), c.user_id, a.id, a.symbol, a.asset_class, a.display_name, 'buy',
       (c.created_at at time zone 'America/Sao_Paulo')::date,
       case when a.asset_class in ('fixed_income', 'intl_fixed_income') and not a.listed_on_b3
            then ci.value else ci.quantity end,
       case when a.asset_class in ('fixed_income', 'intl_fixed_income') and not a.listed_on_b3
            then 1 else ci.unit_price end,
       'plan', c.id, c.created_at
  from contribution_items ci
  join contributions c on c.id = ci.contribution_id
  join assets a on a.id = ci.asset_id
 order by c.created_at, c.id, ci.id;

-- 2, d < 0: o ajuste, depois das compras.
insert into transactions
    (id, user_id, asset_id, symbol, asset_class, display_name, kind, trade_date, quantity, source)
select gen_random_uuid(), a.user_id, a.id, a.symbol, a.asset_class, a.display_name, 'adjustment',
       (now() at time zone 'America/Sao_Paulo')::date, a.quantity - b.quantity, 'migration'
  from assets a
  join ledger_bought b on b.asset_id = a.id
 where a.quantity - b.quantity < 0
 order by a.created_at, a.id;

-- 3: o cache do custo.
update assets a
   set cost_basis = case
           when a.quantity = 0 then 0
           when a.quantity > coalesce(b.quantity, 0) then null
           else b.cost
       end,
       realized_gain = 0
  from assets a2
  left join ledger_bought b on b.asset_id = a2.id
 where a2.id = a.id;

do $$
declare
    wrong integer;
begin
    select count(*) into wrong
      from assets a
      left join (
          select asset_id,
                 sum(case
                         when kind in ('opening', 'buy', 'split', 'bonus', 'adjustment') then quantity
                         when kind in ('sell', 'redemption', 'reverse_split') then -quantity
                         else 0
                     end) as quantity
            from transactions
           where asset_id is not null
           group by asset_id
      ) l on l.asset_id = a.id
     where coalesce(l.quantity, 0) <> a.quantity;
    if wrong > 0 then
        raise exception 'livro nao bate com a quantidade de % ativo(s)', wrong;
    end if;
end
$$;
