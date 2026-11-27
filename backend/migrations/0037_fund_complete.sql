-- FII completo (docs/fundamentos.md): o caixa do fundo e a liquidez da cota.
--
-- O caixa vem do informe mensal da CVM: `Valor_Ativo` (o ativo total, do
-- `complemento`) e `Total_Necessidades_Liquidez` (disponibilidades, titulos
-- publicos, titulos privados e fundos de renda fixa, do `ativo_passivo`).
-- `balance_read` diz que a linha foi gravada por um codigo que ja le esses dois
-- numeros: enquanto houver linha sem a marca no ano de um arquivo, o codigo
-- novo le o arquivo de novo, inteiro (sem o GET condicional). E condicao no
-- dado, nao na migracao: um pod antigo que regrave a linha durante o canario
-- tira a marca, e o novo le de novo, uma vez.
alter table cvm_funds
    add column total_assets numeric,
    add column liquid_assets numeric,
    add column balance_read boolean not null default false;

-- O volume financeiro do pregao (cotas negociadas x fechamento, os dois no pe
-- do Yahoo: o produto nao muda com desdobramento), na moeda do simbolo. Nulo =
-- a fonte ainda nao disse. Gravado pelo mesmo caminho do fechamento
-- (`Store::record_prices`): preenche o que esta nulo e, nos ultimos dias,
-- corrige o que a fonte mudou.
alter table price_history
    add column traded_value numeric(24, 2) check (traded_value >= 0);

-- Desde quando o volume de um simbolo ja foi pedido a fonte, de ponta a ponta
-- ate o dia do pedido. Sem a linha (o simbolo guardado antes do volume), a
-- sincronia pede de novo, uma vez, o trecho da janela da liquidez.
create table traded_value_coverage (
    symbol text primary key,
    covered_from date not null
);
