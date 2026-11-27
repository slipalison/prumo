# Renda: histórico flexível e mapa de calor

A tela Renda responde "quanto minha carteira paga?". Este documento é do
painel **Histórico**: o livro de proventos somado do jeito que a pessoa escolhe
(barras) e o **mapa de calor** mês × ano em reais, DY e yield on cost. A
estimativa de 12 meses, a média de 5 anos, o "ano por mês", "Quem pagou" e os
próximos pagamentos continuam como estão (estimativa em
[proventos-automaticos.md](proventos-automaticos.md), § 9; desenho em
[design.md](design.md), Renda).

Código: regra em `backend/src/domain/income_history.rs` e
`backend/src/domain/income_history/heatmap.rs`, rotas em
`backend/src/http/income_history.rs`, leitura do patrimônio médio em
`backend/src/store/income_history.rs`; tela em
`frontend/src/features/income/IncomeHistory.tsx`, `HistoryBars.tsx` e
`IncomeHeatmap.tsx`. Contrato em [api.md](api.md#histórico-e-mapa-de-calor).
Nenhuma fonte externa nova: tudo sai do livro de proventos, do livro de
lançamentos, das fotos diárias do Desempenho e da classificação setorial que a
Saúde já guarda.

O Prumo **informa, não recomenda**: o painel mostra o que entrou e quanto isso
é do patrimônio e do custo; não diz o que comprar.

## 1. O que entra

Todo provento do livro (`income_events`): lançado à mão, importado da B3 e o
automático. Recebido ou a receber, sempre pela **data de pagamento** contra o
dia de hoje no Brasil, qualquer que seja a data do agrupamento.

### Líquido ou bruto

O valor é o **gravado em cada provento**. O líquido de cada um já seguiu, quando
foi gravado, a mesma regra da estimativa (`domain::income::default_withholding`):

- JCP: 15% retidos até 31/12/2025 e 17,5% desde 1/1/2026, pela data de
  pagamento;
- dividendo e rendimento de ação e REIT dos EUA: 30% retidos lá;
- BDR, FII, dividendo brasileiro, amortização e o resto: nada.

A retenção que a pessoa digitou (inclusive 0) e a que o extrato da B3 trouxe
valem como estão: o líquido é o dinheiro que caiu na conta. O histórico **não
recalcula** o líquido de proventos antigos com a regra de hoje — o que está
gravado é o que aconteceu (`015b542`: "lançamentos já gravados não mudam").

### Data de pagamento ou data com

- **Pagamento**: o mês em que o dinheiro caiu.
- **Data com**: o mês em que o direito foi garantido — o **último pregão antes
  da data ex** (`income_auto::record_date_before`, o inverso de
  `ex_date_after`, pelo calendário de pregões da B3). Não é o dia anterior de
  calendário: a data com de sexta 30/01/2026 tem data ex na segunda 02/02, e o
  dia anterior (domingo, 01/02) poria o provento em fevereiro.
- Sem data ex (lançamento manual que não a informou), vale a data de pagamento.

## 2. Barras

- **Intervalo**: mês, trimestre, semestre ou ano civil. Os intervalos vão do
  primeiro provento ao de hoje (ou ao do último a receber), **sem buraco**: mês
  sem provento aparece vazio.
- **Período à vista** (só na tela): 12 meses, 2 anos (padrão), 5 anos ou tudo —
  os intervalos que começam a partir do intervalo de `N − 1` meses atrás, mais
  os de depois de hoje.
- **Agrupar**:
  - **Total**: o recebido cheio e o a receber listrado.
  - **Classe**: a cor de cada classe, a mesma do resto do app.
  - **Segmento**: nas **ações**, o segmento da B3 ("Bancos", "Energia
    Elétrica"; sem ele, o subsetor e, por fim, o setor); nos **FIIs**, o
    **tipo** que o Prumo tira do balanço (tijolo, papel, fundo de fundos,
    híbrido), porque o segmento que o administrador declara à CVM é
    "Multicategoria" ou "Outros" em cerca de três quartos dos fundos
    ([saude.md](saude.md#fiis-informe-mensal-da-cvm)). As outras classes não
    têm fonte de segmento e aparecem pela classe; ação ou FII que a fonte não
    classificou é "sem segmento".
  - **Ativo**: o pagador, como em "Quem pagou".
  - Ativo **apagado** da carteira continua no histórico: pelo nome no
    agrupamento por ativo e como "Ativos removidos" por classe e segmento (o
    evento guarda o símbolo, não a classe).
- **Cores**: a ordem dos grupos é a do **histórico inteiro** (do que mais pagou
  ao que menos), e a cor segue o grupo, não a posição no período à vista —
  trocar o período não repinta ninguém. Segmento e ativo levam as seis cores de
  "Quem pagou"; o sétimo em diante vai para "Outros (N)".
- **Acessível**: a cor nunca é a única pista. A legenda dá o nome e o total de
  cada grupo no período; o balão de cada barra lista as partes; o a receber é
  listrado; uma tabela escondida traz cada intervalo × grupo para o leitor de
  tela. Com o olho fechado, todo R$ (eixo, legenda, balão e tabela) sai
  mascarado.

## 3. Mapa de calor

Mês × ano, de janeiro do ano do primeiro provento a dezembro do ano de hoje (ou
do último a receber). Data e valor são os mesmos das barras.

### Em reais

O provento do mês, recebido e a receber. O mês que ainda tem algo a receber leva
borda tracejada (e "dos quais R$ X a receber" no texto do leitor de tela).

### DY do mês

```
DY do mês = provento do mês ÷ patrimônio médio do mês × 100
```

- **Patrimônio médio do mês**: a média das fotos diárias da carteira
  (`portfolio_snapshots.market_value`, uma por dia corrido desde o primeiro
  lançamento do livro, refeitas quando o livro muda — [desempenho.md](desempenho.md)).
- A **carteira inteira**, inclusive o que não paga provento (cripto, Tesouro,
  renda fixa): é quanto a carteira toda rendeu em provento naquele mês.
- **Média, e não a foto do fim do mês**: um aporte grande no dia 30 derrubaria o
  DY do mês inteiro; com a média, ele pesa os dias em que esteve lá.
- Mês sem foto não tem DY: antes do primeiro lançamento do livro (por exemplo,
  provento automático de antes do saldo inicial) e depois de hoje.

### Yield on cost do mês

```
yield on cost do mês = provento do mês ÷ custo médio do mês × 100
```

- **Custo médio do mês**: o `cost_basis` do livro (preço médio × quantidade, com
  taxas — o que a pessoa pagou), somado entre os ativos e em média nos **dias do
  mês em que havia alguma posição**, até hoje. A primeira compra no dia 20 não
  divide o custo pelos 19 dias sem nada.
- O **saldo inicial vale para trás**, a mesma regra da quantidade na data com
  (`entitlement::normalized`): o provento que ele gerou antes do cadastro (o
  automático da B3 alcança 12 a 18 meses) tem o custo dele para dividir. Por
  isso um mês pode ter yield on cost e não ter DY.

### O ano

```
DY do ano = Σ provento dos meses com base ÷ média da base desses meses × 100
```

Igual para o yield on cost. É o DY **daquele ano**, nunca anualizado (um ano
com patrimônio só desde junho mostra o rendimento de junho a dezembro). Mês
depois do de hoje não entra (não tem base), e o a receber dele fica só em R$.

### Fica de fora (e a célula diz "parcial")

- **Ativo apagado** da carteira: o patrimônio e o custo dele saíram junto com o
  ativo (as fotos são refeitas sem ele), então o provento dele sai do DY e do
  yield on cost daquele mês. Continua em R$.
- **Custo desconhecido** (saldo inicial sem preço médio) em algum dia do mês: o
  provento e o custo daquele ativo saem do yield on cost do mês — a mesma regra
  do yield on cost da carteira ([api.md](api.md#yield-on-cost)). Livro que não
  fecha (venda além da posição) conta como custo desconhecido em todo mês e vai
  para o log.
- No ano, mês com provento e sem base também deixa o total parcial.

A célula parcial leva um `*` e, no texto do leitor de tela, "(parcial: parte do
provento fica fora da conta)".

### Escala e acessibilidade

- Um tom só (o dourado do dinheiro), em cinco passos pela fração do **maior
  mês** à vista na métrica escolhida; zero e sem base ficam sem cor. O total do
  ano não entra na escala.
- Cada célula traz o **número escrito** (a cor só reforça): R$ com centavos até
  R$ 100, sem centavos até mil e "R$ 1,2 mil" daí para cima; DY e yield on cost
  com duas casas. O leitor de tela ouve o valor exato e a conta ("0,83%: R$
  41,25 do patrimônio médio de R$ 5.000,00"); o mesmo texto vai no balão do
  mouse.
- O texto de cada passo tem 4,5:1 ou mais sobre o fundo dele, nos dois temas
  (`styles/incomeHistory.test.ts`).
- É uma `<table>` de verdade. Abaixo de 1000 px de largura do painel, cada ano
  vira um bloco com os meses em grade (seis por linha; quatro no celular) e o
  nome do mês antes do valor — sem rolagem lateral.
- Com o olho fechado, os reais saem mascarados; DY e yield on cost ficam
  (percentual não é patrimônio, a mesma regra do resto da Renda).

## 4. Limites

- O DY é da **carteira inteira**; não há filtro por classe nem por ativo.
- A fração do mês conta o provento pela data escolhida e a base pela média do
  mês: quem vendeu depois da data com recebe o provento num mês em que a
  posição já não está no patrimônio, e quem comprou no fim do mês pesa pouco na
  média. Num ano inteiro isso se dilui.
- O patrimônio vem das fotos do Desempenho: enquanto a série é refeita (depois
  de um lançamento retroativo), o DY usa as fotos de antes.
- O histórico é do que está no livro. Provento que o livro não tem (anterior ao
  alcance da B3 e não importado) não aparece.

## 5. Exemplo (carteira fictícia)

R$ 1.000 em um FII e R$ 4.000 em uma ação desde 05/01/2026, fotos diárias de
R$ 5.000 em fevereiro e março:

| Mês | Provento líquido | DY | Yield on cost |
|---|---|---|---|
| fev/26 | R$ 10,00 (FII) | 0,2% | 0,2% |
| mar/26 | R$ 41,25 (JCP de R$ 50 bruto) + R$ 20,00 de um ativo apagado | 0,825% (parcial) | 0,825% (parcial) |
| 2026 | R$ 71,25 | 1,025% (parcial) | 1,025% (parcial) |

Pela data com, o rendimento pago em 13/02 (data ex 02/02) é de janeiro, e o JCP
pago em 10/03 (data ex 20/02), de fevereiro. É o cenário de
`backend/tests/income_history.rs`.
