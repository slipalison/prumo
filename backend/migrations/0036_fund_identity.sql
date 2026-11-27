-- Qual fundo e o de um ticker de FII (docs/fundamentos.md, "Ticker de FII ->
-- fundo"). A CVM nao traz o ticker; o Prumo achava o fundo pela raiz do ISIN
-- da cota, e houve fundo novo que preencheu o ISIN de outro. A B3 lista o CNPJ
-- de cada fundo pelo codigo de negociacao: esta tabela guarda o que ela disse
-- (CNPJ nulo = a B3 nao achou o fundo), com a vez de uma replica buscar.
create table b3_fund_cnpj (
    issuer text primary key check (issuer ~ '^[A-Z0-9]{4}$'),
    cnpj text check (char_length(cnpj) <= 20),
    name text check (char_length(name) <= 200),
    fetched_at timestamptz,
    claimed_until timestamptz
);

-- O fundo (CNPJ, como a CVM escreve) de onde saiu cada linha de FII da
-- referencia de setor. Linha sem ele (gravada por um codigo de antes), ou com
-- um CNPJ que nao e o que a B3 diz para o ticker, faz o codigo novo ler o
-- informe de novo: condicao no dado, nao na migracao.
alter table sector_reference add column cnpj text check (char_length(cnpj) <= 20);
