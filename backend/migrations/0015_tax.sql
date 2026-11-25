-- Imposto de renda: o instrumento fiscal de cada ativo, o saldo de prejuízo
-- e de DARF que a pessoa traz de antes do Prumo e os DARFs que ela já pagou.
--
-- A regra (alíquotas, isenções, DARF) mora no código (`domain/tax/`), por
-- ano-calendário; aqui fica só o que é da pessoa.

-- --------------------------------------------------- instrumento fiscal --
--
-- O que o ativo é para a Receita, que não é a classe da carteira: o ETF de
-- bitcoin da B3 é cripto na carteira e ETF no IR; o BDR é ação internacional
-- na carteira e BDR no IR; o ETF de IMA-B é renda fixa e fica fora do IR do
-- Prumo (`unsupported`, como a renda fixa, o Tesouro e o exterior fora de BDR).
--
-- `tax_instrument_custom` diz se a pessoa escolheu. Sem escolha, o valor
-- acompanha o padrão (`TaxInstrument::default_for` em
-- `domain/tax_book.rs`) quando a marca "negociado na B3" ou o nome mudam.
--
-- O padrão dos ativos que já existem sai da MESMA conta do código, escrita
-- aqui em SQL; o teste da migração confere as duas com uma carteira fictícia.

alter table assets
    add column tax_instrument text,
    add column tax_instrument_custom boolean not null default false;

update assets
   set tax_instrument = case
           when asset_class in ('fixed_income', 'intl_fixed_income') then 'unsupported'
           when asset_class = 'crypto' and listed_on_b3 then 'etf'
           when asset_class = 'crypto' then 'crypto'
           when asset_class in ('intl_stocks', 'us_reits') and not listed_on_b3 then 'unsupported'
           -- BDR: código com 31 a 35 ou 39 no fim, ou DRN/DR1/DR2/DR3/DRE no nome.
           when asset_class in ('intl_stocks', 'us_reits', 'br_stocks')
                and (upper(trim(coalesce(symbol, ''))) ~ '^[A-Z]{4}(3[1-5]|39)$'
                     or display_name ~* '(^|\s)(DRN|DR1|DR2|DR3|DRE)(\s|$)')
               then 'bdr'
           -- Fundo de índice de renda fixa: fora.
           when asset_class in ('intl_stocks', 'us_reits', 'br_stocks')
                and (display_name ~* '(fundo|classe) de [iIíÍ]ndice'
                     or display_name ~* '(^|[^[:alnum:]])etf([^[:alnum:]]|$)')
                and display_name ~* '(IMA-B|IMA,B|IMAB|IRF-M|IRFM|TESOURO|RENDA FIXA|SELIC|DEB[EÊê]NTURE)'
               then 'unsupported'
           when asset_class in ('intl_stocks', 'us_reits') then 'etf'
           when asset_class = 'br_stocks'
                and (display_name ~* '(fundo|classe) de [iIíÍ]ndice'
                     or display_name ~* '(^|[^[:alnum:]])etf([^[:alnum:]]|$)')
               then 'etf'
           when asset_class = 'br_stocks' then 'stock'
           -- FIIs: o ETF de IFIX é ETF; com "agro" no nome, Fiagro.
           when display_name ~* '(fundo|classe) de [iIíÍ]ndice'
                or display_name ~* '(^|[^[:alnum:]])etf([^[:alnum:]]|$)'
               then 'etf'
           when display_name ~* 'agro' then 'agribusiness_fund'
           else 'real_estate_fund'
       end;

alter table assets
    alter column tax_instrument set not null,
    add constraint assets_tax_instrument_known check (
        tax_instrument in ('stock', 'etf', 'bdr', 'real_estate_fund', 'agribusiness_fund', 'crypto', 'unsupported')
    );

-- ---------------------------------------------- saldo de antes do Prumo --
--
-- O prejuízo a compensar por grupo e o imposto abaixo do mínimo do DARF
-- ainda não pago, como estavam em 1º de janeiro de `year` (o primeiro ano
-- que o Prumo apura). Informado pela pessoa, uma linha por carteira.

create table tax_carryforward (
    user_id uuid primary key references users (id) on delete cascade,
    year integer not null check (year between 2000 and 2100),
    common_loss numeric(20, 2) not null default 0 check (common_loss >= 0),
    day_trade_loss numeric(20, 2) not null default 0 check (day_trade_loss >= 0),
    real_estate_fund_loss numeric(20, 2) not null default 0 check (real_estate_fund_loss >= 0),
    agribusiness_fund_loss numeric(20, 2) not null default 0 check (agribusiness_fund_loss >= 0),
    -- Código 6015 (bolsa) e 4600 (cripto).
    pending_exchange_darf numeric(20, 2) not null default 0 check (pending_exchange_darf >= 0),
    pending_capital_gain_darf numeric(20, 2) not null default 0 check (pending_capital_gain_darf >= 0),
    updated_at timestamptz not null default now()
);

-- ------------------------------------------------------- DARF pago --
--
-- A marca de "paguei" que a pessoa põe num DARF do mês. `amount` é o valor
-- que o Prumo mostrava quando ela marcou: se o livro mudar depois, a tela
-- avisa que o valor de agora é outro.

create table tax_darf_payments (
    user_id uuid not null references users (id) on delete cascade,
    year integer not null,
    month smallint not null check (month between 1 and 12),
    code text not null check (code in ('6015', '4600')),
    amount numeric(20, 2) not null check (amount > 0),
    paid_at timestamptz not null default now(),
    primary key (user_id, year, month, code)
);
