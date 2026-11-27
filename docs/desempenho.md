# Desempenho: patrimônio no tempo e rentabilidade contra os índices

A tela Desempenho responde "estou ganhando do CDI?". Este documento diz como o
número é calculado, de onde vêm as séries e o que ele ainda não sabe.

A regra pura está em `backend/src/domain/performance.rs` (a cota, os índices,
os períodos, os meses), `backend/src/domain/retro.rs` (o dia refeito a partir
do livro), `backend/src/domain/net_worth.rs` (patrimônio × aportado),
`backend/src/domain/class_returns.rs` (por classe) e
`backend/src/domain/monthly_stats.rs` (melhor e pior mês, contagens); a tarefa
diária, a reconstrução e a leitura, em `backend/src/performance.rs` e
`backend/src/performance/classes.rs`; as fontes, em `backend/src/history/` e
`backend/src/treasury/history.rs`.

## Método de cotas (TWR)

A carteira é tratada como um fundo de uma pessoa só:

1. No primeiro dia da série, a cota vale **1** e a carteira tem
   `patrimônio / 1` cotas.
2. A cada dia seguinte:
   - `cota_hoje = (patrimônio_hoje − fluxo_hoje + proventos_hoje) / cotas_ontem`;
   - `cotas_hoje = patrimônio_hoje / cota_hoje`.
3. A rentabilidade de um período é `cota_fim / cota_base − 1`, com a base no
   último fechamento em ou antes do começo do período.

O fluxo entra **no fim do dia**: primeiro o preço anda sobre as cotas que já
existiam, depois o aporte compra cotas novas pelo valor recém-calculado. Por
isso aportar não muda a rentabilidade — só o preço e os proventos mudam. Isso é
conferido com proptest:

- o aporte comprado pelo preço do dia não muda a cota do dia, seja qual for o
  ativo e a quantidade, nem a de dia nenhum da série refeita do livro;
- aportes na mesma proporção da carteira deixam a série inteira igual;
- o desdobramento (ou grupamento), com o preço que o Yahoo ajustou, não muda
  o patrimônio nem a cota;
- provento recebido aumenta a cota do dia e dali em diante;
- a rentabilidade de uma janela é a composição das janelas que a formam, e a
  tabela mês × ano soma o mesmo que a série inteira.

Casos de borda: carteira sem cotas (vazia, ou só com ativo ainda sem preço)
mantém a cota onde estava até chegar dinheiro; perda total leva a cota a um
piso positivo (1e-12) em vez de zero. A cota do primeiro dia é 1 por
definição: a diferença entre o que se pagou e o fechamento do primeiro dia
aparece no ganho, não na rentabilidade (a série mede a partir do primeiro
fechamento).

### Proventos: retorno, não aporte

O provento líquido recebido (dividendo, JCP já sem o IR retido, rendimento de
FII, leilão de frações) entra **na data de pagamento** como retorno: é dinheiro
que a carteira rendeu e saiu para a pessoa. Na conta da cota ele soma ao
patrimônio do dia (`+ proventos_hoje`) — sem isso, a queda do preço na data
ex viraria perda, e a rentabilidade de FII ficaria abaixo da real.

- Fica fora do **aplicado** (`net_flow`): provento não é dinheiro que a pessoa
  pôs nem tirou. Se ela reinvestir, a compra é que é aporte.
- A tela mostra o **ganho** de preço (patrimônio − aplicado) e, ao lado, os
  **proventos** recebidos desde o começo da série. O ganho total é a soma dos
  dois.
- Vem de `income_events` com `source` manual ou `b3_import` e ativo que ainda
  existe; a **estimativa** não é dinheiro recebido e não entra.
- Amortização: a do livro (`kind = amortization`) é retirada de capital
  (fluxo negativo). Se o mesmo ativo tem um provento do tipo amortização na
  mesma data, ele fica de fora, para o mesmo dinheiro não contar duas vezes.
- Provento pago depois de vender tudo (carteira sem cotas naquele dia) não
  tem cota para aumentar: não entra na rentabilidade.

## A série a partir do livro

A série é uma função do livro de lançamentos: **uma foto por dia corrido**,
desde o lançamento mais antigo até hoje. Importou o extrato da B3, o histórico
aparece desde a primeira negociação; o fim de semana e o feriado repetem o
preço do último pregão. Cada dia (`domain/retro.rs`):

1. **Quantidade** de cada ativo: a soma do livro até o dia (`trade_date`,
   ordem do livro). Desdobramento, grupamento e bonificação são lançamentos e
   mudam a quantidade no dia do evento.
2. **Preço** em reais: o último observado em ou antes do dia. A ordem das
   fontes é:
   1. o fechamento real em `price_history` (Yahoo para B3, EUA e cripto; o
      preço de resgate do CSV do Tesouro para título do Tesouro), vezes o
      câmbio do mesmo dia em outra moeda;
   2. o preço que a foto daquele dia já guardou (a cotação do momento de uma
      foto antiga, o preço manual da época);
   3. só no dia de hoje, o preço que a tela mostra (cotação de agora ou preço
      manual);
   4. ativo de preço manual sem nada antes: o preço manual de hoje.

   Ativo sem preço nenhum fica fora da carteira até ganhar um, e aí entra
   inteiro como fluxo (a falta de preço não vira perda, nem a volta dele,
   ganho). Renda fixa por valor não tem preço: a quantidade do livro já é o
   saldo em reais.
3. **Fluxo** do dia, pelos lançamentos do dia:

   | Lançamento | Fluxo |
   |---|---|
   | Compra (`buy`) | `+ (quantidade × preço + taxas) × câmbio`: o que saiu do bolso |
   | Venda e resgate (`sell`, `redemption`) | `− (quantidade × preço − taxas) × câmbio`: o que voltou |
   | Amortização | `− quantidade × preço × câmbio`; a quantidade fica |
   | Desdobramento, bonificação, grupamento | nenhum: muda a quantidade, não o dinheiro |
   | Saldo inicial (`opening`) de ativo cotado | pelo preço do dia, tenha ou não preço médio (o preço médio antigo é custo para o IR, não o valor do dia) |
   | Saldo inicial de renda fixa por valor | o valor |
   | Ajuste de ativo cotado | pelo preço do dia: é posição que entrou ou saiu por fora |
   | Ajuste de renda fixa por valor | **rendimento**: é a atualização do saldo que a pessoa informou |

   O preço pago acima do fechamento do dia (e a taxa) aparece como pequena
   perda naquele dia, e abaixo, como ganho: é o que de fato aconteceu com o
   dinheiro. A venda que zera a posição fica na foto do dia com quantidade
   zero, para o dia medir o preço da venda e não o de ontem.
4. **Proventos** do dia (acima) e a **cota** pela regra do método.

O "aplicado" do gráfico de patrimônio é a soma dos fluxos desde o primeiro
dia. Ganho = patrimônio − aplicado.

A série refeita segue os ativos que existem: o lançamento de um ativo
removido não entra. Remover um ativo refaz a série desde o primeiro
lançamento dele, como se ele não tivesse existido (para registrar que vendeu,
lance a venda).

### Desdobramento e o `close` do Yahoo

O Yahoo dá dois preços por dia: `close` (ajustado por desdobramento) e
`adjclose` (que desconta também os proventos). O Prumo usa o `close`, porque o
provento entra como provento, com data de pagamento e valor líquido. Mas o
`close` também reescreve o passado: depois de um desdobramento de 1 para 2,
o Yahoo passa a mostrar a metade do preço em todos os dias de antes (e a
bonificação da B3 vem como desdobramento, `105:100`).

**A regra:** o livro tem a quantidade real de cada dia (antes do desdobro, as
cotas de antes), e o patrimônio de um dia é `quantidade real × preço real`.
Converter as quantidades de antes do evento pelo fator (`quantidade × fator`)
e multiplicar pelo `close` ajustado (`preço / fator`) dá o mesmo número — a
conversão é feita do lado do preço, uma vez, quando ele chega:

- o cliente pede o `chart` com `events=split` e o período **até hoje** (o
  evento só vem se cair dentro do período pedido, e um desdobro depois da
  janela ajustou os preços dela);
- cada `close` é multiplicado pelo produto dos fatores
  (`numerator / denominator`) dos eventos **depois** daquele dia: é o preço
  que se pagava no dia;
- `price_history` guarda só esse fechamento real, como já guardava o dia
  baixado na hora. Por isso a conversão não pode ficar para depois: o cache
  mistura dias baixados antes e depois de cada desdobro (`on conflict do
  nothing` não reescreve o passado), e só o preço real é igual nos dois.

O desdobro precisa estar no livro (a importação da B3 traz o "Desdobro"; à mão,
é o lançamento `split` com as cotas recebidas). Sem ele, a quantidade fica a de
antes com o preço novo (a metade, num desdobro de 1 para 2): aparece como
perda no dia do evento.

## Quando a série é refeita

- **A tarefa diária** (`SNAPSHOT_AT`, padrão `19:00` de Brasília, depois do
  fechamento da B3; `off` desliga tudo, inclusive a reconstrução): baixa o
  preço que falta, refaz a série de cada pessoa do dia seguinte à última foto
  até hoje (a foto de hoje é sempre refeita: rodar duas vezes no mesmo dia dá
  o mesmo resultado) e baixa o que falta dos índices. Se o processo sobe
  depois do horário, roda na hora.
- **O livro mudou no passado** (importação da B3, lançamento antigo digitado,
  editado ou apagado, provento, ativo que mudou de símbolo ou de preço
  manual): gatilhos no Postgres (migração `0013`) marcam a série da pessoa
  como suja desde a menor data envolvida (`performance_rebuilds.dirty_since`).
  Marcar no banco pega toda escrita, venha da tela, do "Já comprei" ou da
  importação, sem cada caminho precisar lembrar. Uma tarefa em segundo plano
  olha as marcas a cada 10 s e refaz cada série **só dali em diante**: continua
  da foto do dia anterior à marca (cota e posições), até a última foto ou
  ontem. A marca só é atendida com 3 s de idade, para a importação (centenas
  de lançamentos em sequência) terminar e a série ser refeita uma vez. No fim,
  a marca é apagada se ainda é a mesma que foi lida (uma marca nova no meio do
  caminho fica para a rodada seguinte).
- **Preço que chega para um dia que já passou** (o histórico de um ativo novo,
  o fechamento de ontem do câmbio) marca como suja, daquele dia em diante, a
  série de quem tem o ativo.
- A cotação ao vivo (o evento `prices`) **não** mexe na série: ela muda o
  ponto parcial de hoje, calculado na hora em cada leitura.
- Enquanto a série espera ser refeita, as rotas dizem desde quando
  (`rebuilding_since`) e a tela avisa e pergunta de novo a cada 5 s.

**Duas réplicas:** a tarefa diária e a reconstrução usam o mesmo advisory lock
`JOB_LOCK_KEY` (`pg_try_advisory_lock`, sem esperar): só uma réplica escreve
séries por vez, e a diária e a reconstrução nunca ao mesmo tempo. Quem não pega
o lock pula a rodada e a marca fica para a próxima. O lock fica preso a uma
conexão: se o processo cair, a conexão fecha e o Postgres solta o lock. E se
duas rodadas rodassem uma depois da outra, o mesmo livro dá a mesma série e os
caches só inserem data nova.

**Hoje parcial:** antes da foto do dia, as rotas calculam hoje na hora (da
última foto até hoje, com a cotação do momento onde ainda não há fechamento),
sem gravar, e marcam o ponto com `partial: true`.

## Fontes

| Série | Fonte | Unidade | Guardada em |
|---|---|---|---|
| CDI | BCB/SGS série 12 | % ao dia | `index_history`, `cdi` |
| Selic | BCB/SGS série 11 | % ao dia | `index_history`, `selic` |
| IPCA | BCB/SGS série 433 | % no mês, datado no dia 1º | `index_history`, `ipca` |
| Ibovespa | Yahoo `chart`, `^BVSP` | pontos | `index_history`, `ibov` |
| IFIX | estatísticas históricas da B3 (`GetPortfolioDay`) | pontos | `index_history`, `ifix` |
| Ativos e câmbio | Yahoo `chart` diário com `events=split` (`KNRI11.SA`, `AAPL`, `BTC-USD`, `USDBRL=X`) | moeda do símbolo, fechamento real | `price_history` |
| Título do Tesouro (`TD-IPCA-20350515`) | CSV do Tesouro Transparente, preço de resgate (`PU Venda Manha`) | reais | `price_history` |

Hosts: `api.bcb.gov.br`, `query1.finance.yahoo.com`,
`www.tesourotransparente.gov.br` (os dois já usados pela cotação) e
`sistemaswebb3-listados.b3.com.br` (o do IFIX, já usado pelos setores e
proventos). Nenhum pede chave nem contato; o cliente HTTP é o do app, que se
identifica de forma genérica.

**Só o que a carteira usa, só o que falta.** Para cada símbolo com livro, de
qualquer carteira, o histórico começa no lançamento mais antigo dele **ou** no
começo da janela mais longa do risco (5 anos e 10 dias atrás,
`domain::risk::history_start`), o que vier antes — a tela de risco mede a
volatilidade do ativo, não só a do período em que a pessoa o tem
([`risco.md`](risco.md)). O do câmbio começa no mais antigo dos ativos naquela
moeda. A sincronia pede à fonte:

- **para trás**, o que falta antes do que já foi pedido. `price_coverage`
  guarda até onde a fonte já foi consultada, para o símbolo que não tem dado
  tão antigo (fonte que não guarda, ticker que mudou) não ser pedido de novo
  a cada rodada;
- **para a frente**, o que vem depois do último dia guardado.

Os desdobramentos que vêm no mesmo pedido vão para `price_splits` (migração
0030, a tabela do comparador), com a conferência em `price_split_checks`: o
risco e o comparador medem retorno no pé de hoje com eles. Símbolo sem a
conferência desde o começo do histórico é pedido uma vez de ponta a ponta. Só
a data que **entrou** de fato em `price_history` suja a série de quem tem o
ativo (pedir de novo um trecho já guardado não refaz nada).

**Fechamento ruim da fonte** (um trecho de 1 a 5 pregões a 2× ou à metade do
preço de antes, que volta a até 20% dele, sem desdobramento): não entra em
`price_history`, e o que já estava guardado sai (na entrada, quando a volta
chega, e numa conferência do cache inteiro ao subir e na tarefa diária), com a
série de quem tinha o ativo refeita. Regra e exemplos em
[`risco.md`](risco.md#fechamento-ruim-da-fonte).

Ativo de preço manual não é pedido (o manual vence a cotação).

**Tesouro Direto:** o histórico do título sai do mesmo CSV da cotação do dia
(uns 15 MB, de 2004 até a última divulgação), lido em fluxo, linha a linha
(`treasury/history.rs`): só as linhas dos títulos que alguma carteira tem, a
partir do primeiro lançamento de cada um (o título sem cupom, desde o começo
da janela longa do risco, como os outros ativos), vão para `price_history` — o
resto passa e é esquecido. O arquivo é baixado **só quando falta dia de antes** do
que já está guardado (um download para todos os títulos que faltam). Para a
frente, a sincronia diária do Tesouro (a que grava a cotação em `quotes`)
grava também o preço do dia de cada título que alguma carteira tem: a série
anda sem baixar a tabela de novo. O preço de hoje, antes da divulgação, é a
cotação do dia.

**BCB/SGS** (medido com `curl` em 25/11/2026):
`/dados/serie/bcdata.sgs.N/dados?formato=json&dataInicial=dd/mm/aaaa&dataFinal=dd/mm/aaaa`
responde `[{"data":"18/11/2026","valor":"0.050788"}]`. Série diária aceita no
máximo 10 anos por pedido (`406` acima disso): o cliente pagina em janelas de
3.600 dias. Janela sem dado publicado (o IPCA do mês corrente) responde `404`,
e o cliente trata como lista vazia. Os índices se estendem para trás sozinhos
quando a série passa a começar mais cedo; o Ibovespa começa, no mínimo, no
começo da janela longa do risco.

**IFIX, estatísticas da B3** (medido com `curl` em 27/11/2026, com o cliente do
app): a página pública "Estatísticas históricas" do IFIX no site da B3 lê
`GET https://sistemaswebb3-listados.b3.com.br/indexStatisticsProxy/IndexCall/GetPortfolioDay/<base64>`,
com `{"index":"IFIX","language":"pt-br","year":"2025"}` em base64 no caminho —
sem login, chave, cookie nem CAPTCHA. A resposta (uns 9 KB) é um ano por
pedido: `results` traz uma linha por dia do mês (`day`, 1 a 31) com
`rateValue1`…`rateValue12`, o fechamento de cada mês naquele dia em texto
brasileiro (`"3.117,34"`), `null` sem pregão. Ano antes do índice responde
`{"min":null,"max":null,"results":null}`; o IFIX começa em 30/12/2010 com
1.000 pontos, e ano anterior a 2010 nem é pedido. O pregão em andamento não
aparece (às 14h de 27/11 o último dia era 26/11), e o Ibovespa da mesma
tabela bate com o fechamento do Yahoo. A sincronia pede um ano de cada vez,
com meio segundo entre eles: a primeira vez, os anos desde a primeira foto de
qualquer carteira (uns 8 pedidos para uma carteira de 2019); depois, só o ano
corrente, uma vez por dia, na tarefa das 19h com os outros índices. Falha da
B3 só deixa o IFIX de ontem: os outros índices e a série seguem. A B3 é quem
calcula o índice: é a fonte oficial dele.

**Yahoo `chart`** com `period1`/`period2`, `interval=1d` e `events=split`: a
data do pregão vem do `timestamp` no fuso da bolsa (`meta.gmtoffset`). O pregão
em andamento (antes do fim do horário regular,
`currentTradingPeriod.regular.end`) fica de fora: o "fechamento" dele ainda é
o preço do momento.

## Índices no período

- **CDI e Selic** (taxa diária): `Π (1 + taxa/100) − 1` sobre as datas `d` com
  `base <= d < fim`. A taxa da data `d` rende de `d` para o dia útil seguinte;
  o CDI de hoje só é publicado amanhã, e `as_of` diz até onde foi.
- **IPCA** (mensal): cada mês inteiro na janela entra inteiro; mês pela metade
  entra pela fração de dias corridos, de forma composta
  (`(1 + taxa)^(dias/dias_do_mês)`). Mês ainda não publicado fica de fora, e
  `as_of` diz o último dia coberto.
- **Ibovespa e IFIX** (nível): último fechamento em ou antes do fim sobre o
  último em ou antes da base.
- Índice sem dado na janela vem `null` (a linha some do gráfico), nunca zero.

**"x% acima do CDI"** é a diferença composta, `(1 + carteira) / (1 + CDI) − 1`:
é o quanto a carteira rendeu a mais que o CDI no mesmo período. Ao lado, a
rentabilidade em porcentagem do CDI (`carteira / CDI`, "118% do CDI"), só
quando o CDI subiu.

## Períodos

`month` (desde o fechamento do último dia do mês passado), `ytd` (desde 31/12),
`12m` (desde a mesma data, 12 meses atrás), `2y` e `5y` (a mesma data, 24 e 60
meses atrás) e `all` (desde a primeira foto).
Quando a série é mais nova que o período, a janela começa na primeira foto, e
o cartão de 12 meses diz "a série ainda não tem 12 meses".

A tabela mês × ano mede cada mês do último fechamento do mês anterior (ou da
primeira foto, no mês em que a série começa) ao último fechamento do mês; o
ano é a composição dos meses, e ao lado vai o CDI da mesma janela.

## Patrimônio × aportado

A curva do patrimônio contra o **aportado acumulado** (a soma dos fluxos do
livro desde o primeiro dia: compras com taxas menos vendas, resgates e
amortizações), e a ponte do período (`domain/net_worth.rs`):

`patrimônio no fim = patrimônio na base + aportes líquidos + mercado`

- **Base**: o fechamento de base do período (o mesmo da rentabilidade). Quando
  a janela começa na primeira foto (o período "Tudo", ou uma série mais nova
  que o período), a base é o **zero** de antes do primeiro lançamento: o
  aportado é o aplicado inteiro e o mercado, o ganho de preço inteiro.
- **Aportes**: os fluxos depois da base até o fim (negativo = retirou mais do
  que pôs).
- **Mercado**: o resto, `fim − base − aportes`: o que o preço fez com o
  dinheiro, incluindo a diferença entre o preço pago e o fechamento do dia.
- **Proventos**: o recebido no período. O provento sai da carteira para a
  pessoa, então **não** está no patrimônio; o que ela reinvestiu entrou como
  aporte. O resultado do período é mercado + proventos.

A ponte fecha por construção, e duas janelas seguidas somam a janela inteira
(aportes, mercado e proventos) — conferido com proptest. No caso fictício
abaixo: desde o começo, aportado R$ 2.281,50, mercado R$ 123,50, proventos
R$ 10,00; da base de D2 (R$ 2.840,00), aportes −R$ 519,50 (a venda), mercado
+R$ 84,50 e o rendimento de R$ 10,00.

Os atalhos de período são os da tela (Mês, Ano atual, 12 meses, 2 anos, 5 anos,
Tudo) e valem para todos os painéis.

## Por classe

O peso de cada classe e a rentabilidade dela no período, ao lado do índice da
classe (`domain/class_returns.rs`, `performance/classes.rs`).

**Método: cada classe é uma carteira só dela.** Os ativos da classe (a classe
de hoje de cada ativo), com o mesmo livro, os mesmos preços (`price_history`,
o que as fotos guardaram e, hoje, o preço do momento) e os proventos dos ativos
dela, são refeitos dia a dia pela mesma regra da carteira (`retro::rebuild`),
começando do fechamento de base do período com a cota em 1. Por isso:

- **sem viés de aporte**: o aporte na classe compra cotas pelo valor do dia e
  não mexe na cota; aportar muito num mês bom não infla o retorno;
- provento é retorno da classe, na data de pagamento, como na carteira;
- classe que **entrou** no meio do período mede do primeiro dia em que teve
  dinheiro ("desde 26/07/2025" na tela); a que **zerou** mede até o dia da
  venda ("até 10/03/2026"), e a cota para ali;
- classe que não teve dinheiro em nenhum dia do período não aparece.

**Peso**: o patrimônio da classe no fim do período sobre o da carteira (a
soma das classes). **Índice da classe**, na mesma janela da classe: IFIX para
FIIs, Ibovespa para Ações Nacionais, CDI para Renda Fixa; as outras classes
não têm um índice no Prumo e ficam sem (Ações e REITs dos EUA, cripto, renda
fixa internacional).

Conferido com proptest: com uma classe só, a rentabilidade da classe medida
do zero na base é a da carteira na mesma janela (a série desde o primeiro
lançamento); sem aporte nem provento na janela, a rentabilidade da carteira é
a média das classes pesada pelo patrimônio de cada uma na base. De ponta a
ponta (`tests/performance.rs`, fictício): 10 KNRI11 a R$ 142 e um CDB de
R$ 1.000 no dia 1; no dia 2, 5 KNRI11 compradas a R$ 148 com o fechamento a
R$ 150 e o CDB a R$ 1.010. FIIs: `(2.250 − 740) / 1.420 − 1` = **+6,338%**,
peso 2.250 / 3.260 = 69,02%; Renda Fixa: **+1%**, peso 30,98%; a carteira,
**+4,1322%** (2.520 / 2.420); o IFIX falso subiu 1% no mesmo dia.

A série das classes é refeita **na hora da leitura**, sem gravar nada (para
"Tudo", desde o primeiro lançamento): é a mesma conta da reconstrução, uma vez
por pedido.

## Ano a ano

A tabela mês × ano (`monthly_cells`): cada mês mede do último fechamento do mês
anterior ao último do mês (o mês corrente, até hoje); o ano é a composição dos
meses.

- **Modo Ano**: a rentabilidade de cada mês e a do ano.
- **Modo Acumulado**: desde a primeira foto até o fim de cada mês (e do
  ano), `cota no fim do mês / primeira cota − 1`. Mostrado com uma casa (os
  números são maiores; duas casas não cabem no iPad em pé).
- **Índices nas mesmas janelas**: para cada mês da carteira, o CDI, o IPCA, o
  Ibovespa e o IFIX acumulados na janela daquele mês (e desde a primeira foto,
  no acumulado). Índice sem dado na janela (o IPCA do mês corrente) fica
  `null`, nunca zero; a tela diz até quando ele saiu.
- **Melhor e pior mês**: a maior e a menor rentabilidade mensal da série (no
  empate, o mês mais antigo), contando o mês corrente.
- **Meses positivos, negativos e no zero**: todo mês cai em uma das três.
- **Acima do índice**: em quantos meses a carteira rendeu mais que o índice na
  mesma janela, entre os meses em que o índice tinha dado (empate não conta).

É informação, não nota: nenhum mês é "bom" ou "ruim" fora do número.

## Conferência passo a passo

O caso fictício abaixo foi feito à mão e é conferido duas vezes: pela regra
pura (`domain/retro.rs`, `hand_computed_case`) e de ponta a ponta, com Postgres,
o `chart` do Yahoo falso (com o preço de WEGE3 já ajustado pelo desdobro) e as
rotas (`tests/retro.rs`).

**O livro** (março de 2025):

| Dia | Lançamento |
|---|---|
| D1, seg 03/03 | compra de 10 KNRI11 a R$ 100,00, taxa R$ 1,00; CDB fictício, saldo inicial de R$ 1.000,00 |
| D2, ter 04/03 | compra de 20 WEGE3 a R$ 40,00 |
| D3, qua 05/03 | KNRI11 paga R$ 10,00 de rendimento (líquido) |
| D4, qui 06/03 | WEGE3 desdobra de 1 para 2 (`split` de 20 cotas); o CDB rende R$ 5,00 (ajuste) |
| D5, sex 07/03 | venda de 5 KNRI11 a R$ 104,00, taxa R$ 0,50 |

**Os fechamentos.** KNRI11: 101, 102, 103, 103, 104. WEGE3, como o Yahoo
mostra depois do desdobro: 20,50, 21,00, 21,50, 22,00 — o real de D2 e D3 é o
dobro, 41 e 42.

**Cada dia.** Patrimônio (P), fluxo (F), proventos (R) e a cota
(`cota = (P − F + R) / cotas de ontem`; com `cotas = P / cota`, isso é a cota
de ontem vezes `(P − F + R) / P de ontem`):

| Dia | Patrimônio | Fluxo | Prov. | Cota |
|---|---|---|---|---|
| D1 | 10 × 101 + 1.000 = **2.010** | 10 × 100 + 1 + 1.000 = **2.001** | 0 | **1** |
| D2 | 10 × 102 + 20 × 41 + 1.000 = **2.840** | 20 × 40 = **800** | 0 | (2.840 − 800) / 2.010 = **1,0149254** |
| D3 | 10 × 103 + 20 × 42 + 1.000 = **2.870** | 0 | **10** | 1,0149254 × 2.880 / 2.840 = **1,0292200** |
| D4 | 10 × 103 + 40 × 21,50 + 1.005 = **2.895** | 0 (desdobro e ajuste não são dinheiro) | 0 | 1,0292200 × 2.895 / 2.870 = **1,0381854** |
| D5 | 5 × 104 + 40 × 22 + 1.005 = **2.405** | −(5 × 104 − 0,50) = **−519,50** | 0 | 1,0381854 × 2.924,50 / 2.895 = **1,0487645** |
| sáb, dom | 2.405 (preço de sexta) | 0 | 0 | 1,0487645 |

- Rentabilidade desde o início: `1,0487645 / 1 − 1` = **+4,8765%**
  (`(2.040/2.010) × (2.880/2.840) × (2.924,50/2.870) − 1`; o 2.895 se cancela).
  Março de 2025 na tabela mês × ano: o mesmo +4,8765%; os meses seguintes, 0.
- Aplicado: 2.001 + 800 − 519,50 = **R$ 2.281,50**. Ganho de preço:
  2.405 − 2.281,50 = **R$ 123,50**. Proventos: **R$ 10,00**.
- Prova do ganho total (R$ 133,50): KNRI11 custou 1.001 e devolveu
  520 (as 5 que ficaram) + 519,50 (a venda) + 10 (o rendimento) = +48,50;
  WEGE3 custou 800 e vale 880 = +80; o CDB rendeu +5.

**Depois de mexer no passado** (o mesmo teste de ponta a ponta):

- a compra de WEGE3 de D2 corrigida para R$ 39,00: o fluxo de D2 vira 780 e a
  cota de D2, 2.060 / 2.010; D1 não é tocado (a mesma foto, da mesma hora);
- o rendimento de D3 apagado: D3 fica só com o preço, e a cota de D5 vira
  `(2.060/2.010) × (2.924,50/2.840)`;
- a venda de D5 apagada: D5 fica com 10 KNRI11, patrimônio 2.925 e fluxo 0;
- uma compra de KNRI11 em 28/02 faz a série começar em 28/02 (o fechamento
  desse dia é baixado para trás); apagada, a série volta a começar em D1 e o
  dia que sobrou some.

## Limitações

- **O livro precisa estar completo.** O que não está no livro não está na
  série: compra feita fora do Prumo e não lançada, desdobro não registrado
  (vira perda no dia do evento). A importação da B3 traz negociações e eventos
  desde ~2019, sem corretagem: a taxa da compra importada é zero.
- **Renda fixa por valor** só rende quando a pessoa lança o ajuste do saldo;
  sem isso, aparece parada. Não há indexador automático.
- **Ticker que mudou** (incorporação, troca de código): o Yahoo costuma ter o
  histórico só sob o código novo. Sem preço antes, o ativo entra na série no
  primeiro dia com preço, como fluxo.
- **Câmbio e cripto** usam o fechamento do dia do Yahoo quando ele já existe;
  às 19h o pregão do câmbio (até 23:59 GMT) ainda não fechou, e hoje vale a
  cotação do momento (o fechamento chega no dia seguinte e refaz o dia).
- O Yahoo não é fonte oficial (já é a fonte da cotação). IVVB11 (S&P 500 em
  reais) como índice fica de fora por ora; o IFIX vem da B3.
- **Por classe** usa a classe de hoje de cada ativo: ativo que mudou de classe
  leva o histórico inteiro para a classe nova. A série das classes é refeita a
  cada leitura (não fica gravada); numa carteira de muitos anos, "Tudo" é a
  leitura mais pesada da tela.
- **IFIX**: se a B3 mudar o formato da página de estatísticas, o índice para de
  andar (o que já está guardado fica) e o log diz `indice nao baixado`.
