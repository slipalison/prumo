-- Projeção por cenários: as expectativas do Focus (Banco Central) e as
-- premissas que a pessoa pode trocar em cada objetivo. Método inteiro em
-- `docs/projecao.md`.

-- As medianas do Focus que a projeção usa, da pesquisa mais recente que a
-- busca diária achou. `period`: `annual` (a referência é o ano, gravado no
-- dia 1º de janeiro) ou `monthly` (o mês, gravado no dia 1º). A Selic anual é
-- a meta esperada no fim do ano; o IPCA anual é a variação no ano; o IPCA
-- mensal, a variação no mês. Tudo em %, como o Focus publica.
--
-- Uma busca que falha não apaga nada: a linha de ontem continua valendo, com a
-- data da pesquisa dela (`survey_date`).
create table focus_expectations (
    indicator text not null check (indicator in ('ipca', 'selic')),
    period text not null check (period in ('annual', 'monthly')),
    reference date not null check (extract(day from reference) = 1),
    median numeric(10, 4) not null,
    survey_date date not null,
    primary key (indicator, period, reference),
    constraint focus_expectations_annual_starts_the_year
        check (period <> 'annual' or extract(month from reference) = 1),
    constraint focus_expectations_selic_is_annual
        check (indicator <> 'selic' or period = 'annual')
);

-- Uma linha só: quando a última busca deu certo e até quando uma réplica tem a
-- vez de buscar (a outra pula). A busca não segura conexão nem lock durante a
-- rede; a gravação é uma transação curta.
create table focus_sync (
    singleton boolean primary key default true check (singleton),
    fetched_at timestamptz,
    claimed_until timestamptz
);

-- Premissas da projeção, por objetivo. Nulo = o padrão que a resposta da API
-- explica (`docs/projecao.md`). Tudo em % ao ano.
--
-- `real_return` substitui `expected_return`: a premissa de rentabilidade passa
-- a ser REAL (acima da inflação). O valor que a pessoa tinha informado era
-- nominal e vira real descontando o IPCA dos 12 últimos meses que o banco tem
-- (`index_history`), a mesma série do Desempenho:
-- `real = (1 + nominal) / (1 + ipca_12m) - 1`. Sem os 12 meses de IPCA no
-- banco não há como converter, e a premissa volta ao padrão (nulo).
alter table objectives
    add column real_return numeric(7, 4)
        check (real_return >= -50 and real_return <= 50),
    add column inflation numeric(7, 4)
        check (inflation >= -10 and inflation <= 50),
    add column reinvest_income boolean,
    add column income_growth numeric(7, 4)
        check (income_growth >= -50 and income_growth <= 50),
    add column income_yield numeric(7, 4)
        check (income_yield >= 0 and income_yield <= 50),
    add column scenario_spread numeric(7, 4)
        check (scenario_spread >= 0 and scenario_spread <= 20),
    -- Só na meta de renda mensal: `average` (a média por mês) ou `floor` (o
    -- menor mês de 12). Nulo = `average`.
    add column income_mode text
        check (income_mode in ('average', 'floor')),
    add constraint objectives_income_mode_only_for_income
        check (income_mode is null or kind = 'monthly_income');

with last_12 as (
    select value from index_history where code = 'ipca' order by date desc limit 12
), ipca as (
    select case when count(*) = 12 then exp(sum(ln(1 + value / 100))) end as factor
      from last_12
)
update objectives o
   set real_return = greatest(-50, least(50, round(((1 + o.expected_return / 100) / ipca.factor - 1) * 100, 4)))
  from ipca
 where o.expected_return is not null and ipca.factor is not null;

alter table objectives drop column expected_return;
