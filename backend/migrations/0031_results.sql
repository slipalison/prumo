-- Resultados trimestrais da carteira (docs/resultados.md): a serie mensal dos
-- FIIs e uma releitura dos arquivos de companhia. Tudo e REFERENCIA
-- compartilhada entre as carteiras, como o resto da 0020. A data de entrega de
-- cada DFP e ITR vem de `cvm_filing_receipts` (0028, da Agenda).

-- Cada mes do informe mensal de FII (`inf_mensal_fii_complemento`), dos fundos
-- com cota negociavel: o patrimonio, as cotas e o rendimento do mes sobre o
-- valor patrimonial (`Percentual_Dividend_Yield_Mes`, fracao), como o fundo
-- informou. A versao maior de cada mes vence.
create table cvm_fund_months (
    cnpj text not null check (char_length(cnpj) <= 20),
    -- `Data_Referencia`: o primeiro dia do mes.
    reference_date date not null,
    version integer not null check (version >= 0),
    net_assets numeric,
    shares numeric,
    dividend_yield numeric,
    updated_at timestamptz not null,
    primary key (cnpj, reference_date)
);

-- Releitura. Os documentos ja gravados nao tem a conta de equivalencia
-- patrimonial (DRE 3.04.06, que diz se a companhia e holding):
-- a lista de codigos lidos vazia faz a proxima rodada ler cada arquivo de
-- companhia de novo, inteiro e sem GET condicional (`Mode::Extend` de todas).
-- O informe mensal sem `checked_at` e lido de novo inteiro (`Mode::Full`) e
-- traz a serie. O que estava gravado continua valendo ate la.
update cvm_sources
   set scanned_codes = '{}'
 where source like 'annual:%' or source like 'quarterly:%';

update cvm_sources
   set checked_at = null
 where source like 'fund_monthly:%';
