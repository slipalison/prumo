# Desempenho: patrimônio no tempo e rentabilidade contra os índices

A tela Desempenho responde "estou ganhando do CDI?". Este documento diz como o
número é calculado, de onde vêm as séries e o que ele ainda não sabe.

A regra pura está em `backend/src/domain/performance.rs`; a tarefa diária e a
leitura, em `backend/src/performance.rs`; as fontes, em `backend/src/history/`.

## Método de cotas (TWR)

A carteira é tratada como um fundo de uma pessoa só:

1. Na primeira foto, a cota vale **1** e a carteira tem `patrimônio / 1` cotas.
2. A cada foto seguinte:
   - `cota_hoje = (patrimônio_hoje − fluxo_hoje) / cotas_ontem`;
   - `cotas_hoje = patrimônio_hoje / cota_hoje`.
3. A rentabilidade de um período é `cota_fim / cota_base − 1`, com a base no
   último fechamento em ou antes do começo do período.

O fluxo entra **no fim do dia**: primeiro o preço anda sobre as cotas que já
existiam, depois o aporte compra cotas novas pelo valor recém-calculado. Por
isso aportar não muda a rentabilidade — só o preço muda. Isso é conferido com
proptest:

- o aporte comprado pelo preço do dia não muda a cota do dia, seja qual for o
  ativo e a quantidade;
- aportes na mesma proporção da carteira deixam a série inteira igual;
- a rentabilidade de uma janela é a composição das janelas que a formam, e a
  tabela mês × ano soma o mesmo que a série inteira.

Casos de borda: carteira sem cotas (vazia, ou só com ativo ainda não comprado)
mantém a cota onde estava até chegar dinheiro; perda total leva a cota a um
piso positivo (1e-12) em vez de zero.

### O que é fluxo e o que é rendimento

Cada foto guarda as posições (ativo, quantidade e preço em reais). A foto
seguinte compara com elas:

| Mudança entre as fotos | Entra como |
|---|---|
| Aporte registrado ("Já comprei") | fluxo, pelo valor **pago** |
| Quantidade de ativo cotado mudada à mão (ativo novo, edição, ativo apagado) | fluxo, pelo preço do dia: é compra ou venda feita fora do Prumo |
| Ativo cotado que ganhou ou perdeu a cotação | entra ou sai inteiro, para a falta de preço não virar perda |
| Valor de renda fixa (ativo por valor) que já estava na carteira | **rendimento**: é a variação informada pela pessoa |
| Renda fixa que aparece ou some | fluxo, inteira |
| Variação de preço | rendimento |

O aporte pago acima do fechamento do dia aparece como pequena perda naquele dia
(e abaixo, como ganho): é o que de fato aconteceu com o dinheiro.

O "aplicado" do gráfico de patrimônio é a soma dos fluxos desde a primeira
foto; a primeira foto entra inteira como aplicado (o Prumo não sabe o custo de
antes dela — ver [Limitações](#limitações)). Ganho = patrimônio − aplicado.

## A foto diária

- **Quando:** todo dia às `SNAPSHOT_AT` (padrão `19:00`, horário de Brasília,
  depois do fechamento da B3 e da publicação do pregão no Yahoo). `off`
  desliga. Se o processo sobe depois do horário, roda na hora (um deploy à
  noite não perde o dia).
- **O que faz, em ordem:**
  1. baixa o que falta do fechamento de cada símbolo cotado de alguma carteira
     e do câmbio das moedas deles (`price_history`);
  2. tira a foto de cada pessoa que tem ativo ou série
     (`portfolio_snapshots`), com o fechamento do dia quando existe e, senão,
     o preço que a tela mostra (cotação de agora ou preço manual). Título
     do Tesouro não existe no Yahoo: fica fora do `price_history` (como fica
     fora do streamer) e vale pela cotação do dia em `quotes`;
  3. baixa o que falta dos índices, desde 10 dias antes da primeira foto
     (`index_history`).
- **Idempotente:** a foto é um `upsert` por (pessoa, dia). Rodar duas vezes no
  mesmo dia refaz a foto com o estado mais novo; não cria outra. Os aportes de
  uma foto são os registrados entre a hora da foto anterior (`taken_at`) e a
  desta.
- **Duas réplicas:** as duas agendam no mesmo horário. Quem roda é quem pega o
  advisory lock `JOB_LOCK_KEY` (`pg_try_advisory_lock`, sem esperar); a outra
  vê o lock ocupado e pula. O lock fica preso a uma conexão: se o processo
  cair (ou a tarefa for cancelada sem soltar), a conexão é fechada e o
  Postgres solta o lock. E se as duas rodassem uma depois da outra, o
  `upsert` e os caches que só inserem data nova dão o mesmo resultado.
- **Hoje parcial:** antes da foto do dia, as rotas calculam a foto de hoje na
  hora, sem gravar, e marcam o ponto com `partial: true`.

## Fontes

| Série | Fonte | Unidade | Guardada em |
|---|---|---|---|
| CDI | BCB/SGS série 12 | % ao dia | `index_history`, `cdi` |
| Selic | BCB/SGS série 11 | % ao dia | `index_history`, `selic` |
| IPCA | BCB/SGS série 433 | % no mês, datado no dia 1º | `index_history`, `ipca` |
| Ibovespa | Yahoo `chart`, `^BVSP` | pontos | `index_history`, `ibov` |
| Ativos e câmbio | Yahoo `chart` diário (`KNRI11.SA`, `AAPL`, `BTC-USD`, `USDBRL=X`) | moeda do símbolo | `price_history` |
| Título do Tesouro (`TD-IPCA-20350515`) | a cotação do dia em `quotes` (preço de resgate do CSV do Tesouro Transparente) | reais | — (a foto guarda o preço) |

Hosts: `api.bcb.gov.br` (novo) e `query1.finance.yahoo.com` (já usado pela
cotação). Nenhum pede chave nem contato; o cliente HTTP é o do app, que se
identifica de forma genérica.

**BCB/SGS** (medido com `curl` em 25/11/2026):
`/dados/serie/bcdata.sgs.N/dados?formato=json&dataInicial=dd/mm/aaaa&dataFinal=dd/mm/aaaa`
responde `[{"data":"18/11/2026","valor":"0.050788"}]`. Série diária aceita no
máximo 10 anos por pedido (`406` acima disso): o cliente pagina em janelas de
3.600 dias. Janela sem dado publicado (o IPCA do mês corrente) responde `404`,
e o cliente trata como lista vazia.

**Yahoo `chart`** com `period1`/`period2` e `interval=1d`: a data do pregão
vem do `timestamp` no fuso da bolsa (`meta.gmtoffset`). O pregão em andamento
(antes do fim do horário regular, `currentTradingPeriod.regular.end`) fica de
fora: o "fechamento" dele ainda é o preço do momento.

### Fechamento ajustado ou não: o Prumo guarda o `close`

O Yahoo dá dois preços por dia: `close` (ajustado só por desdobramento) e
`adjclose` (que desconta também os proventos, reescrevendo o passado a cada
dividendo). O Prumo guarda o **`close`**:

- o patrimônio é quantidade × preço que se pagava no dia; com o `adjclose`, o
  patrimônio de ontem mudaria a cada dividendo novo e não bateria com o que a
  pessoa tinha;
- a foto do dia fica congelada com o preço do dia; o `adjclose` obrigaria a
  refazer a série inteira a cada provento;
- provento tem de entrar como provento (com data de pagamento e valor
  líquido), e não embutido no preço — é o que a frente de proventos (F3) traz.

O cache não reescreve data já guardada (`on conflict do nothing`): se um papel
desdobrar depois, os dias guardados continuam com o preço de antes, que é o
preço real daquele dia. Só um download novo de dias antigos viria com o
`close` já ajustado pelo desdobramento — a reconstrução retroativa precisa
tratar isso (ver abaixo).

## Índices no período

- **CDI e Selic** (taxa diária): `Π (1 + taxa/100) − 1` sobre as datas `d` com
  `base <= d < fim`. A taxa da data `d` rende de `d` para o dia útil seguinte;
  o CDI de hoje só é publicado amanhã, e `as_of` diz até onde foi.
- **IPCA** (mensal): cada mês inteiro na janela entra inteiro; mês pela metade
  entra pela fração de dias corridos, de forma composta
  (`(1 + taxa)^(dias/dias_do_mês)`). Mês ainda não publicado fica de fora, e
  `as_of` diz o último dia coberto.
- **Ibovespa** (nível): último fechamento em ou antes do fim sobre o último em
  ou antes da base.
- Índice sem dado na janela vem `null` (a linha some do gráfico), nunca zero.

**"x% acima do CDI"** é a diferença composta, `(1 + carteira) / (1 + CDI) − 1`:
é o quanto a carteira rendeu a mais que o CDI no mesmo período. Ao lado, a
rentabilidade em porcentagem do CDI (`carteira / CDI`, "118% do CDI"), só
quando o CDI subiu.

## Períodos

`month` (desde o fechamento do último dia do mês passado), `ytd` (desde 31/12),
`12m` (desde a mesma data, 12 meses atrás) e `all` (desde a primeira foto).
Quando a série é mais nova que o período, a janela começa na primeira foto, e
o cartão de 12 meses diz "a série ainda não tem 12 meses".

A tabela mês × ano mede cada mês do último fechamento do mês anterior (ou da
primeira foto, no mês em que a série começa) ao último fechamento do mês; o
ano é a composição dos meses, e ao lado vai o CDI da mesma janela.

## Limitações

- **A série nasce no primeiro dia em que a tarefa roda.** Não há passado: a
  primeira foto entra inteira como "aplicado", sem o custo real de antes.
- **Proventos ainda não contam.** Dividendo e rendimento de FII pagos em
  dinheiro saem da carteira sem registro, então a rentabilidade é só de preço
  — para FIIs, fica abaixo da real.
- **Renda fixa por valor** só rende quando a pessoa atualiza o valor; sem
  isso, aparece parada. Não há indexador automático (o Tesouro automático é
  outra frente).
- **Venda** ainda não existe no Prumo: quantidade reduzida à mão entra como
  retirada pelo preço do dia.
- **Câmbio e cripto** usam o fechamento do dia do Yahoo quando ele já existe;
  às 19h o pregão do câmbio (até 23:59 GMT) ainda não fechou, e vale a cotação
  do momento.
- O Yahoo não é fonte oficial (já é a fonte da cotação). IFIX e IVVB11 como
  índice ficam de fora por ora.

## O que a reconstrução retroativa (onda 2) precisa do livro

Com o livro de lançamentos (frente F1) e a importação da B3 (F2), a série pode
começar no primeiro lançamento. Para isso:

1. **Quantidade por ativo em cada data**, a partir dos lançamentos
   (`trade_date`, `kind`, `quantity`), incluindo desdobramento, grupamento e
   bonificação — a foto de cada dia é `Σ quantidade(d) × close(d)`.
2. **Fluxo por data**: compra = `quantity × unit_price + fees`, venda =
   `−(quantity × unit_price − fees)`, amortização e resgate como saída;
   `opening` sem custo conhecido entra pelo preço do dia (como a primeira foto
   de hoje).
3. **Proventos recebidos** (F3, `income_events` com `pay_date` e `net`): na
   data de pagamento, entram como fluxo negativo (o dinheiro sai da carteira
   como renda), o que soma o provento à rentabilidade.
4. **Preço de cada dia desde o primeiro lançamento**: baixar o `close` do
   Yahoo para trás (o `sync_prices` hoje só pede uma semana). Como o Yahoo
   ajusta o `close` passado por desdobramento, a quantidade do livro antes do
   evento precisa ser convertida pelo fator do desdobramento (ou pedir
   `events=split` e desfazer o ajuste no preço).
5. **Renda fixa**: valor por data (o que a pessoa informou, ou o indexador).
   **Tesouro Direto**: o histórico do título pode vir do próprio CSV do
   Tesouro Transparente, que traz o preço de resgate de todos os dias desde o
   lançamento — sem o Yahoo.
6. Os índices já se estendem sozinhos para trás: `sync_indexes` busca o que
   falta antes do primeiro dia guardado quando a primeira foto fica mais
   antiga.

As fotos retroativas são gravadas no mesmo `portfolio_snapshots` (o `upsert`
por dia permite refazer), e a foto diária continua igual a partir de então.
