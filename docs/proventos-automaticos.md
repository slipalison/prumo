# Proventos automáticos

O Prumo calcula sozinho os proventos de cada ativo da B3 da carteira: o
**anúncio da empresa** (valor por cota, data com, data de pagamento) vezes a
**quantidade do livro na data com**. O provento entra na Renda com o selo
"automático (B3)", recebido ou a receber pela data de pagamento, e conta no
caixa, no desempenho, no IR e no alerta "provento cai amanhã" como qualquer
outro.

O que a pessoa lançou à mão (`manual`) ou importou do extrato (`b3_import`)
**sempre vence**: o automático igual não é criado, e o importado que chega
depois toma o lugar dele.

Código: regra em `backend/src/domain/income_auto.rs`, busca e rodada em
`backend/src/income_auto.rs`, banco em `backend/src/store/income_auto.rs`,
rotas em `backend/src/http/income_auto.rs`, migração `0018_income_auto.sql`.

## 1. Fonte

Páginas públicas de eventos corporativos da B3, sem login, em
`sistemaswebb3-listados.b3.com.br` (o mesmo host da classificação setorial da
Saúde). O parâmetro é o JSON da consulta em base64, no caminho:

| O quê | Rota | Consulta |
|---|---|---|
| Empresa (ação, unit, BDR) | `GET /listedCompaniesProxy/CompanyCall/GetListedSupplementCompany/<b64>` | `{"issuingCompany":"ITSA","language":"pt-br"}` |
| Fundo (FII, ETF) | `GET /fundsProxy/fundsCall/GetListedSupplementFunds/<b64>` | `{"typeFund":7,"cnpj":"0","identifierFund":"KNRI"}` |
| Cadastro (ticker → ISIN) | `GET /listedCompaniesProxy/CompanyCall/GetDetail/<b64>` | `{"codeCVM":"7617","language":"pt-br"}` |

Conferido com `curl` em 26/11/2026; as respostas de ITSA e KNRI estão gravadas
em `backend/tests/fixtures/b3_announcements/` e os testes do cliente rodam
contra elas (wiremock).

- A página da empresa vem como lista de um objeto; a do fundo, como o objeto.
  As duas trazem `cashDividends[]` (`isinCode`, `label`, `rate` em pt-BR com
  11 casas, `lastDatePrior` = data com, `paymentDate`, `approvedOn`,
  `relatedTo`) e `stockDividends[]` (`label`, `factor`, `lastDatePrior`).
- Rótulos vistos em 26/11/2026 (20 empresas, 15 fundos): `DIVIDENDO`,
  `JRS CAP PROPRIO`, `RENDIMENTO`; em ações, `BONIFICACAO`, `DESDOBRAMENTO`,
  `GRUPAMENTO`, `CIS RED CAP`, `RESG TOTAL RV`.
- O `typeFund` 7 é o do FII; a B3 devolve a mesma página para ETF (conferido
  com BOVA11, NDIV11, DIVD11), e a de empresa também responde para fundo. O
  Prumo usa a de fundo para FII e ETF e a de empresa para o resto.
- Emissor inexistente não é erro na B3: volta uma página vazia.
- A B3 às vezes não responde (três pedidos seguidos esperaram 30 s em
  26/11/2026). A busca usa 20 s de tempo limite, uma pausa de 0,8 s entre
  pedidos e tenta o emissor que falhou de novo só depois de 15 minutos.
- Requisição pelo `http_client()` do app (identificação genérica, nenhum dado
  da pessoa). Nada de contornar bloqueio.

### Código → ISIN

ON, PN e unit vêm da mesma empresa e se distinguem pelo ISIN. O código de
negociação (`ITSA4`) vira ISIN assim, nesta ordem:

1. **Cadastro da B3** (`GetDetail`, campo `otherCodes [{code, isin}]`), pelo
   código CVM que a própria página da empresa traz. Ex.: ITSA3 →
   `BRITSAACNOR0`, ITSA4 → `BRITSAACNPR7`, TAEE11 → `BRTAEECDAM10`, AAPL34 →
   `BRAAPLBDR004`. Guardado em `b3_listings`.
2. **Regra do tipo de papel**, entre os ISIN que o emissor anunciou: `BR` +
   emissor + tipo + série + dígito, com o tipo pelo número do código — 3
   `ACNOR` (ON), 4 `ACNPR` (PN), 5 `ACNPA`, 6 `ACNPB`, 7 `ACNPC`, 8 `ACNPD`,
   11 `CTF` em fundo (a cota; o recibo de subscrição `R…` fica de fora) e
   `CDAM`/`UNT` em unit, 31 a 39 `BDR`. Só vale quando **um** ISIN bate:
   dois candidatos é dúvida, e dúvida não vira provento. Fundo não tem
   cadastro público com ISIN (`GetDetailFund` responde 404), então FII e ETF
   usam sempre a regra.

O emissor são os quatro primeiros caracteres do código (`B3SA3` → `B3SA`); o
fracionário (`ITSA4F`) conta como o código cheio.

## 2. Busca diária e rodada

Uma tarefa em segundo plano roda **a cada hora** (a primeira logo depois do
deploy):

1. **Busca.** Os emissores de todos os ativos da B3 de todas as carteiras
   (Ações Nacionais, FIIs e o que estiver marcado "negociado na B3": ETF,
   BDR). Cada emissor cuja última busca tem mais de 20 h é **reivindicado**
   por um `update` condicional em `b3_issuer_fetches` (quem marca primeiro
   busca; a outra réplica pula) e buscado. Os anúncios vão para
   `b3_cash_announcements` e `b3_stock_announcements` por upsert idempotente,
   numa transação curta com `pg_advisory_xact_lock` do emissor.
2. **Rodada.** Para cada pessoa com a chave ligada, o plano do domínio
   (`income_auto::plan`) com o que está no banco, gravado numa transação curta
   sob `pg_advisory_xact_lock` da pessoa. Só o que mudou é escrito.

Duas réplicas, quatro conexões cada: **nenhuma conexão fica presa durante a
rede**. Um lock de sessão (`pg_try_advisory_lock`, como o da tarefa de
desempenho) prenderia uma das quatro durante a busca inteira; por isso a vez
de buscar é a reivindicação, e os locks são todos de transação.

O anúncio que a B3 deixou de listar **dentro do período que ela ainda cobre**
(data com a partir da mais velha da resposta) sai do banco — foi corrigido ou
cancelado, e o automático dele some na rodada. O mais velho que isso fica: a
janela da B3 anda, e o histórico da pessoa não pode encolher junto. Página
vazia não apaga nada.

Sem `QUOTES_ENABLED`, a busca não roda; a rodada roda (o livro pode mudar no
passado).

## 3. A regra

### Data com e quantidade

A quantidade é a **posição do livro no fim da data com** (`lastDatePrior`),
pela mesma função do provento lançado à mão (`entitlement::quantity_on`):
compra datada na data ex não dá direito; o saldo inicial (`opening`) conta
desde sempre (é a quantidade do dia do cadastro). Quantidade zero: nada.

A data ex gravada é o primeiro dia útil depois da data com (calendário de
`domain/tax/calendar.rs`; fora dele, só o fim de semana pula). Ela é
informativa: a quantidade sai da data com.

### Tipo e valores

| Rótulo da B3 | `kind` | IR retido |
|---|---|---|
| `DIVIDENDO` | `dividend` | nenhum |
| `JRS CAP PROPRIO` | `jcp` | `jcp_withholding_rate(pay_date)`: 15% até 2025, 17,5% desde 2026 |
| `RENDIMENTO` em FII | `reit_income` | nenhum |
| `RENDIMENTO` em ação ou ETF | `other` | nenhum (ver limites) |
| começa com `AMORTIZ` | `amortization` | nenhum |
| qualquer outro | `other` | nenhum |

`amount_per_unit` = `rate` (gravado com 8 casas), `gross = rate × quantidade`
com as 11 casas da B3, em centavos; `net = gross − withholding`.

`source = b3_announcement`; `external_ref` estável,
`b3a:<isin>:<rótulo>:<data com>:<valor>` (ex.:
`b3a:BRITSAACNPR7:JRS-CAP-PROPRIO:2026-08-31:0.0242425`). O valor faz parte
da chave porque a mesma empresa anuncia dois JCP com a mesma data com e
valores diferentes (um por trimestre de referência). A data de pagamento não
entra, para a empresa poder mudá-la sem o automático virar outro.

**Parcelas.** Um provento pago em parcelas vem da B3 numa linha por parcela,
com o mesmo ISIN, rótulo, data com e valor; só a data de pagamento muda (o
dividendo de 12/2025 da WEGE3 em três pagamentos anuais, os JCP de 12/2025 da
B3SA3 em quatro). Cada parcela é um anúncio (`b3_cash_announcements` tem a
data de pagamento na chave, com "a definir" valendo como um valor) e um
automático: a primeira, pela data de pagamento, fica com o `external_ref` de
sempre, e as outras ganham `:2`, `:3`... no fim
(`b3a:BRWEGEACNOR0:DIVIDENDO:2025-12-19:0.412831673:2`). A parcela ainda sem
data fica por último, para as de antes não mudarem de número quando ela
ganhar data. Duas linhas idênticas da B3 (mesmo ISIN, rótulo, data com,
valor e data de pagamento) contam como uma.

### Sem data de pagamento

Anúncio com a data "a definir" (vazia, ilegível ou o `31/12/9999` da B3) é
**guardado e não vira provento**. Inventar a data poria o dinheiro no mês
errado da Renda, no caixa e no IR. Quando a empresa define a data, a busca do
dia seguinte a grava e a rodada cria o provento. Em 26/11/2026 nenhum anúncio
das 35 páginas conferidas (20 empresas, 15 fundos) estava sem data.

## 4. Sem duplicar o que já existe

Em produção há proventos lançados à mão (vindos de outra ferramenta) e
importados da B3. Um automático **não é criado** quando já existe, do mesmo
ativo (pelo id, ou pelo código se o ativo foi apagado):

- mesmo **tipo** — dividendo, rendimento e "outro" contam como iguais (a
  importação da B3 chama tudo de Rendimento, e outras ferramentas lançam
  rendimento de FII como dividendo); JCP e amortização são cada um o seu;
- mesma **data**: a data com (o lançado pode guardar a data com ou a data ex
  no campo `ex_date`: qualquer dia entre as duas conta) **ou** a mesma data de
  pagamento (quando o lançado não tem data ex, só sobra essa);
- mesmo **valor por cota, a até 1%** (cobre o arredondamento de 2 casas). No
  JCP também vale o valor por cota **líquido** (o extrato e muitas planilhas
  dão o líquido). Lançado sem valor por cota: compara o líquido ou o bruto, a
  1%.

A comparação é um para um: um provento lançado cobre um anúncio só. Primeiro
pela data de pagamento, em todos os anúncios do ativo, e só depois pela data
com: a parcela de 2027 lançada à mão não cobre a de 2026, que tem a mesma data
com.

**Parcelas lançadas de um anúncio de uma linha.** Há empresa que a B3 mostra
numa linha só e que paga em parcelas: a AMBEV anunciou em 12/2025 um JCP de
0,269 com pagamento "até 31/12/2026" e pagou 0,075 + 0,0755 + 0,1185 em
abril, julho e outubro. Quem lançou as três parcelas não vê o automático de
0,269 em dobro: o anúncio que sobra sem par é coberto por dois ou mais
proventos lançados, ainda sem par, do mesmo tipo e da mesma data com, cuja
soma tem o valor dele (o mesmo 1%, e no JCP também o líquido). Essa busca
vem por último, para um grupo nunca tomar o provento que outro anúncio
casaria sozinho.

**Manual e importado sempre vencem.** A rodada nunca escreve neles (os
`update`/`delete` dela filtram `source = 'b3_announcement'`). Se o automático
já existe e o igual chega depois:

- **lançado à mão**: a rodada seguinte apaga o automático;
- **importado da B3**: a prévia da importação marca a linha
  `replaces_automatic` e a gravação troca o automático pelo importado no mesmo
  registro (como o "recebido substitui o provisionado", `domain/b3_plan.rs`):
  mesma data de pagamento e o mesmo valor a 1%. A importação e a rodada da
  pessoa entram uma de cada vez (o mesmo lock de transação). Se a prévia não
  reconheceu o par, a rodada seguinte apaga o automático.

**Apagar um automático é definitivo**: o `external_ref` fica em
`income_auto_dismissed` e a rodada não o recria (é o caminho quando o
automático está errado, por exemplo cota alugada). Editar um automático não
pode (`422`, como todo provento que não é manual).

## 5. Recalcular

O automático é **derivado**: a cada rodada (de hora em hora) o conjunto da
pessoa é refeito a partir dos anúncios e do livro.

- Lançamento retroativo, editado ou apagado antes da data com → a quantidade
  é refeita (o automático é trocado no mesmo registro).
- Posição zerada na data com, anúncio corrigido ou cancelado, ativo apagado →
  o automático some.
- Livro que não fecha (venda além da posição) → os automáticos daquele ativo
  ficam como estão até o livro ser corrigido.
- Rodada repetida sem mudança não escreve nada: os gatilhos da série de
  desempenho (migração 0013) só sujam a série quando um provento muda de fato.

A rodada é de hora em hora e não pelo gatilho de "sujo" da série retroativa:
aquele gatilho marca a série de desempenho, que é refeita por outra tarefa; a
rodada dos automáticos é barata (só banco) e confere todo mundo a cada hora.

## 6. Eventos em ações

Bonificação, desdobramento e grupamento **não vão para o livro**: a
bonificação pede o custo atribuído, que a página da B3 não traz. Eles viram
uma **pendência** em Ajustes › Proventos automáticos e num cartão da Renda:
"A B3 anunciou bonificação de 2% em ITSA4 com data com 18/12/2025; lance no
livro." O fator é como a B3 publica: percentual na bonificação e no
desdobramento (100 = cada ação vira duas), multiplicador no grupamento (0,1 =
dez viram uma).

Fica de fora o evento: com data com há mais de 548 dias (18 meses) ou no
futuro; anterior ao saldo inicial (que já o inclui); sem cota na data com; já
lançado (um lançamento do mesmo tipo entre a data com e 90 dias depois).
`CIS RED CAP` e `RESG TOTAL RV` não viram pendência.

## 7. Chave por pessoa

Ligada por padrão (`users.income_auto`, migração 0018: quem já existe ganha os
automáticos na primeira rodada depois do deploy). Desligar apaga os
automáticos da pessoa na hora; ligar os refaz na hora com o que já está no
banco, sem ir à B3. Contrato em [api.md](api.md#proventos-automáticos).

## 8. Limites

- **Histórico de 12 a 18 meses.** É o que a B3 mostra. Provento mais velho que
  isso continua com a pessoa lançando ou importando o extrato.
- **Data a definir.** Só entra quando a empresa define (seção 3).
- **ETF.** A distribuição do ETF (rótulo `RENDIMENTO`) entra como "outro" e
  sem IR retido: há ETF que retém IR na fonte e a B3 não informa. O extrato
  importado substitui pelo valor recebido.
- **BDR.** O dividendo do BDR entra como dividendo, sem retenção no Brasil
  (o valor da B3 já é o repassado, depois do imposto lá fora). O IR do BDR é
  pelo carnê-leão, fora daqui.
- **Rendimento de ação** (`RENDIMENTO` numa empresa) é a correção do valor
  entre a data com e o pagamento; entra como "outro", sem retenção.
- **Cota alugada ou em garantia** recebe diferente do livro: apague o
  automático e lance o que recebeu (ou importe o extrato).
- **Emissor que a B3 não cobre** ou código que a regra do ISIN não resolve:
  fica sem automático, e Ajustes lista o código.

## 9. Estimativa da carteira de hoje

Os mesmos anúncios alimentam a **estimativa** da tela Renda ("Estimativa da
carteira de hoje"), do plano do Aportar ("+~R$ X/mês de renda") e da meta de
renda enquanto nada foi recebido. Regra em
`backend/src/domain/income/estimate.rs`; leitura do banco em
`backend/src/income.rs` e `Store::b3_income_sources`.

### Líquida, com a bruta ao lado

A estimativa é o que cada cota pagou nos últimos 12 meses, **líquido do IR
na fonte do JCP**, vezes a quantidade de hoje, ÷ 12. É a mesma base do
"Recebido em 12 meses" e da "Média por mês" (o livro guarda o líquido) e da
meta de renda: as três falam do dinheiro que cai na conta.

- JCP: 15% pago até 31/12/2025, 17,5% desde 1/1/2026, **pela data de
  pagamento** (`jcp_withholding_rate`); "a definir" usa a alíquota de hoje (o
  pagamento ainda vai cair).
- Dividendo, rendimento de FII, amortização e o resto: sem retenção.
- A bruta vem ao lado (menor): no total, em cada ativo e na folha "Estimativa
  por ativo". O imposto lá fora do dividendo americano não é descontado (o
  Prumo não sabe se a pessoa compensa); o que ela receber de fato vai para o
  livro com a retenção que ela informar.

### Fonte: B3 para ações e FIIs, Yahoo de reserva

| Ativo | Fonte | Por quê |
|---|---|---|
| Ação e FII da B3 (`br_stocks`, `br_reits`) | anúncios da B3 (`b3_cash_announcements`) | oficial, separa JCP de dividendo, tem as parcelas |
| B3 sem o ativo (ISIN não resolvido, emissor nunca buscado) | Yahoo | reserva |
| B3 fora do ar (última busca do emissor há mais de 3 dias) | Yahoo, se houver em cache; senão a B3 velha | reserva |
| EUA, BDR, ETF | Yahoo | a B3 não traz, ou o rótulo não diz o IR |

O Yahoo não separa o JCP: o que vem dele é bruto e líquido ao mesmo tempo, e a
tela diz "Yahoo" ao lado do ativo. Medido numa carteira real em 27/11/2026, a
troca para a B3 tirou os buracos do Yahoo (um mês de FII sem rendimento, um
JCP de banco faltando, centavos diferentes) e deixou a estimativa bruta a
centavos da B3 refeita à mão.

### Base de cotas de hoje

O valor da B3 é por cota **da época**. Cada bonificação e desdobramento
(`1 + fator ÷ 100`) e cada grupamento (`× fator`) com data com **no mesmo
dia ou depois** da data com do provento, e data ex até hoje, divide o valor
(`shares_after`). No mesmo dia conta: quem tinha a cota na data com recebe o
dinheiro pelas cotas velhas e as novas chegam depois (uma emissora deu JCP e
bonificação com a mesma data com). O Yahoo já vem nessa base (ele trata
bonificação como desdobramento). Com a quantidade de hoje, que já inclui as
cotas bonificadas, a conta fecha.

### A janela: 365 dias por data ex

`(hoje − 365 dias, hoje]`, **pela data ex**, nas duas fontes. A data ex da B3 é
o primeiro dia útil depois da data com (`ex_date_after`); o Yahoo informa a
própria data ex, e numa carteira real todas as datas ex do Yahoo na janela
eram exatamente o pregão seguinte à data com da B3. Assim trocar de fonte não
mexe na borda.

- **FII mensal com 11 ou 13 pagamentos.** A data ex anda com os dias úteis:
  em alguns dias do ano dois pagamentos do "mesmo mês" cabem nos 365 dias (a
  data com de fevereiro caiu na sexta 27/02/2026 e na sexta 26/02/2027: em
  01/03/2027 as duas datas ex estão na janela), e em outros nenhum (maio de
  2026 com data ex em 29/05, maio de 2027 em 31/05: em 30/05/2027 a janela tem
  11). Numa carteira real isso acontece de 3 a 6 dias por ano e mexe uns 0,2%
  no total. É método, e foi mantido: a janela por mês civil erra mais (um FII
  de meados do mês ficaria com 11 durante metade do mês). O teste
  `a_monthly_reit_can_have_eleven_or_thirteen_payments_in_the_window` mostra
  os dois casos.
- **Fim de ano.** 31/12 não tem expediente e 1/1 é feriado: a data com de
  30/12 tem data ex no primeiro dia útil de janeiro. O calendário é o de
  `domain::tax::calendar` (expediente bancário), que trata 24/12 como dia útil;
  a B3 não negocia em 24/12, então uma data com de 23/12 tem data ex 24/12 no
  Prumo e 26/12 no Yahoo. Só muda a borda em 24 e 25/12.
- **Parcelas.** Cada parcela é um anúncio (seção 1) e conta pelo próprio valor,
  inclusive as que serão pagas nos anos seguintes: o que a cota "pagou" nos 12
  meses é o que foi declarado com data com neles.

### Acima do habitual

O número principal **continua a soma de 12 meses**: é o que de fato foi pago,
e explicável. Ao lado, cada ativo mostra a **média anual de até 5 anos**, por
cota de hoje, vezes a quantidade de hoje ÷ 12, e a marca **"acima do
habitual"** quando os 12 meses passam bem dela. Nada é cortado por regra: a
B3 não marca o que é extraordinário (os rótulos de dezembro de 2025 são os de
sempre, "Anual/2025", "Complementar/2025"), e as regras de corte medidas
(evento acima de 2 ou 3 vezes a mediana, teto pela mediana anual) cortavam
JCP recorrente de banco.

- **A média** (`usual`): janelas de 365 dias para trás. A de agora são os 12
  meses da própria fonte (para a média e o número principal contarem o mesmo
  ano); as mais velhas vêm do Yahoo (a B3 só cobre uns 12 meses). Uma janela
  velha só entra **inteira**, isto é, se o Yahoo tem pagamento de antes do
  começo dela (a regra do Bazin): o ano em que o ativo ainda não existia, ou
  que o histórico não alcança, não vira zero. Com menos de 2 anos não há
  média (a tela diz "histórico curto"), e com menos de 5 a tela diz quantos.
  O histórico do Yahoo passou a ser pedido em 10 anos (`dividends.rs`): com 5,
  o primeiro pagamento é sempre depois do começo do quinto ano, e a média (e o
  Bazin) nunca passava de 4 anos.
- **A marca**: 12 meses > **1,5 ×** a média anual, no bruto (o IR não muda o
  que é "acima"). Como a média inclui o ano de agora, com 5 anos isso equivale
  a 12 meses > ~1,71 × a média dos 4 anos anteriores. O limiar foi medido
  numa carteira real em 27/11/2026: pega as antecipações de dezembro de 2025
  (de 1,5 a pouco mais de 2 vezes a média) e deixa de fora quem só cresceu
  devagar.
- **No total**, a tela diz quanto da estimativa líquida vem de ativos marcados
  ("~R$ X/mês de N ativos acima do habitual"), com o ⓘ: pagamentos acima do
  habitual, como antecipações, e a estimativa pode cair quando eles saírem da
  janela.
- A média líquida usa a parte de JCP dos 12 meses do ativo (o Yahoo não
  separa): é aproximada, e a marca não depende dela.

### Com o livro: o yield on cost estimado

`trailing_12m` (o que a posição teria rendido nos 12 meses com as cotas que o
livro tinha em cada data com) segue a mesma fonte: pela B3 é o valor da época
vezes as cotas da data com, **líquido**, arredondado como o provento
automático; pelo Yahoo, a conta de antes (bruta, na base do provedor). É o que
o yield on cost usa quando nada foi recebido.
