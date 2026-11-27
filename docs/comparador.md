# Comparador de ativos

Tela `#/analise/comparar` (link no topo da Análise). Escolha de até 5 ativos,
da carteira ou da busca de ticker (a mesma do cadastro), guardada no endereço
(`?ativos=PETR4,VALE3`): dá para voltar e para mandar o link. Contrato em
`docs/api.md` (§ Comparador). Não recomenda nada.

## Tabela lado a lado

Só indicadores que o Prumo já calcula: os fundamentos da CVM (`fundamentos.md`),
o DY de 12 meses, o P/L e o P/VP (preço sobre LPA e VPA, ou sobre o valor
patrimonial por cota), o tipo do fundo pelo balanço e o setor da B3.

- **Destaque neutro.** Cada indicador tem uma direção fixa (maior ou menor valor
  da linha), dita na dica da tela. Marca só entre ativos do mesmo tipo (empresa
  com empresa, fundo com fundo) e só com 2+ valores. Não há ranking, nota nem
  palavra de juízo; o marcador diz "maior" ou "menor" em texto.
- **"—" com o motivo.** Indicador que não existe: gap da CVM,
  `no_price_history`, `short_history`, `no_dividend_history`...
- **"Não se aplica", escrito.** Linha de fundo para empresa (`not_applicable`) e
  o indicador que enganaria em banco, seguradora (`financial_template`) e
  holding de participações (`holding`): a margem EBITDA de uma holding passaria
  de 200% e viraria o "maior da linha". A regra é a mesma dos fundamentos, da
  Análise e dos resultados (`CompanyMetrics::holding` e o plano de contas,
  [resultados.md](resultados.md#não-se-aplica-holding-banco-e-seguradora)): a
  célula vem sem número, então fica fora do destaque, e embaixo do bloco das
  empresas uma linha por ativo diz por quê.
- **Ativo fora da carteira.** Preço histórico e proventos vêm do Yahoo na hora e
  ficam no cache (`price_history`, `dividend_history`). Fundamentos só se já
  estiverem no banco; senão `not_tracked`. Nunca dispara a CVM.
- DY = proventos brutos com data ex nos últimos 365 dias ÷ preço. Retorno de
  12 meses = preço, sem proventos, ajustado por desdobramento. Ao lado do nome
  da linha do DY, o CDI de 12 meses depois do IR e o juro real da NTN-B de
  referência, só como contexto (sem destaque, sem "acima" nem "abaixo").
- **FII completo** (linhas de fundo, contas em
  [fundamentos.md](fundamentos.md#fii-completo-liquidez-crescimento-e-rendimento)):
  caixa do informe mensal (sem destaque: muito caixa pode ser dinheiro parado,
  pouco é menos folga; o número não tem lado), liquidez média de 30 e 90
  pregões e CAGR dos proventos por cota e da cota em 3 e 5 anos (destaque no
  maior). Para o fundo, o comparador guarda 5 anos de preço (o CAGR de 5 anos),
  e o volume vem no mesmo pedido do preço.

## Rentabilidade comparada

Curvas normalizadas em 0% na data base (último fechamento em ou antes do início
do período; série que começa depois abre em 0% no próprio começo). Períodos de
1 mês a máximo (no máximo, do primeiro dado dos ativos).

- **Com proventos** (retorno total): o provento bruto entra na data ex e é
  reinvestido no preço do dia. JCP vem junto com o dividendo (fonte Yahoo).
  Sem a chave, só o preço.
- **Igualar o início**: todas as curvas partem da data em que todas existem.
- **Carteira**: a cota do Desempenho, que **sempre** inclui os proventos
  recebidos; a tela avisa quando a chave está desligada.
- **Índices**: CDI (acumulado das taxas diárias) e Ibovespa. O que faltar antes
  da primeira foto da carteira é buscado na hora (BCB e Yahoo) e guardado em
  `index_history`.
- Ativo em dólar ou cripto vai a reais pelo câmbio `USDBRL=X` de cada dia.

## Desdobramentos (migração 0030)

`price_history` guarda o fechamento **real** do dia. Para medir retorno de um
ativo que desdobrou, o comparador guarda os eventos do Yahoo (`price_splits`) e
divide o preço de antes pelo fator. Cada símbolo é conferido no máximo uma vez
por dia (`price_split_checks`), numa só chamada que também completa o preço.
`HistoryProvider::closes_and_splits` tem implementação padrão sem eventos. A
sincronia de preços do Desempenho também grava os eventos e a conferência de
todo ativo com livro, todo dia ([`risco.md`](risco.md)): o ativo da carteira
já chega conferido. O preço que o comparador busca passa pelo mesmo caminho de
gravação (`record_prices`): a barra do pregão aberto não entra e os últimos
dias que a fonte corrigiu são acertados.

## Limites

- Fonte única de preço e provento (Yahoo): dado pode faltar ou vir atrasado.
- Dividendos do Yahoo são brutos e sem data de pagamento.
- O primeiro pedido de um ativo novo demora o que o Yahoo demorar (até 15 s por
  símbolo, em paralelo); os seguintes saem do cache.
