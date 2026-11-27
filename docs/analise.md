# Análise da carteira

A Análise olha a carteira além da meta por classe, em quatro dimensões:
**Diversificação**, **Dividendos**, **Rentabilidade** e **Qualidade dos
ativos**. Ela evolui a antiga Saúde da carteira ([saude.md](saude.md)): as sete
regras que já existiam continuam, nas dimensões certas, e a configuração de
cada pessoa é preservada.

A regra pura está em `backend/src/domain/analysis.rs` e nos submódulos
`domain/analysis/{returns,profile,highlights,references}.rs`; a junção com o
banco, em `backend/src/analysis.rs`; a rota, em `backend/src/http/analysis.rs`.
O contrato está em [api.md](api.md) e a tela, em [design.md](design.md).

## O que é e o que não é

- **A pontuação é simples e está à mostra.** A nota de uma dimensão é o
  **percentual das regras ligadas, com dado, que estão ok**, e a tela escreve o
  número exato ("5 de 8 regras ok", 62,5%). Não há peso, nota composta nem
  "nota do Prumo".
- **Sem dado não é falha.** Série curta, poucos meses de provento, ativo sem
  custo no livro, classe sem fonte de setor: a regra fica `no_data`, **fora** da
  pontuação e **fora** dos "pontos a ver". Ela é dita como "Sem dado", com o
  motivo, e nunca como "tudo certo".
- **Regras do Prumo.** Os limites de fábrica são pontos de partida; a pessoa liga,
  desliga e muda cada um. Nada aqui copia nota, layout, texto, "preço justo" ou
  "upside" de outra ferramenta, nem a metodologia de pontuação de terceiros.
- **Não recomenda ativo de fora da carteira** e **não manda vender**. O "o que
  fazer" é aportar (o valor, em reais, quando dá para calcular), olhar uma tela
  do Prumo ou rever uma decisão.
- **Não diz "oportunidade" nem "compre".** As referências de preço são rotuladas
  "referência pública, não é recomendação".

## As regras

"Ativo" é o que tem **valor** (`value > 0`). Os percentuais são comparados já
arredondados em duas casas, os mesmos que a tela mostra, e "no limite exato"
está ok. Cada regra liga e desliga sozinha e, quando tem limite, ele é editável
(`PUT /analysis/rules`). O número de agora aparece **mesmo dentro do limite**.

### 1. Diversificação

| Regra (`rule`) | Pergunta | Limite de fábrica |
|---|---|---|
| `max_asset` | Algum ativo passa de X% da carteira? | 15% |
| `top_five` | Os 5 maiores juntos passam de Y%? | 60% |
| `max_sector` | Algum setor passa de Z% da classe (ações e FIIs)? | 40% |
| `min_assets` | Alguma classe com meta tem menos de N ativos? | 3 |
| `empty_class` | Alguma classe com meta não tem ativo? | sem limite |
| `min_portfolio_assets` | A carteira tem pelo menos N ativos? | 10 |
| `min_classes` | O dinheiro está em pelo menos N classes? | 3 |
| `min_sectors` | As ações estão em pelo menos N setores? | 4 |
| `min_abroad` | Pelo menos X% da carteira no exterior? | 10% |
| `min_low_risk` | Pelo menos Y% da carteira em renda fixa? | 10% |

As cinco primeiras são as da Saúde ([saude.md](saude.md) tem os detalhes, como
o `top_five` só valer com mais de 5 ativos). As outras:

- **`min_portfolio_assets`**: conta os ativos com valor. Sem ativos, sem dado.
- **`min_classes`**: conta as classes (das sete) que têm algum ativo com valor.
  A resposta lista quais.
- **`min_sectors`**: setores distintos entre as **Ações Nacionais** (setor da
  B3). Ação sem setor na fonte não conta; sem ação com setor, sem dado.
- **`min_abroad`**: fatia de Ações Internacionais, REITs e Renda Fixa
  Internacional. BDR e ETF de exterior negociados na B3 ficam em Ações
  Internacionais no Prumo e entram. Cripto não é exterior.
- **`min_low_risk`**: fatia de Renda Fixa e Renda Fixa Internacional.
- Nas duas de fatia mínima, o aviso traz o aporte que leva a fatia ao limite
  **sem vender nada**: `(L·T − 100·p) ÷ (100 − L)`, com `p` o valor do grupo, `T`
  o total e `L` o limite em %, arredondado para cima em centavos. Com limite de
  100% não há aporte que chegue lá e o valor não é dito.

### 2. Dividendos

| Regra | Pergunta | Limite de fábrica |
|---|---|---|
| `min_yield` | O dividend yield de 12 meses é de pelo menos X%? | 5% |
| `min_paying_share` | Pelo menos Y% dos ativos de renda variável pagaram em 12 meses? | 70% |
| `income_growth` | A renda de 12 meses é maior que a dos 12 anteriores? | sem limite |
| `income_every_month` | Caiu provento em quantos dos 12 meses fechados? | 12 meses |
| `max_payer` | Algum pagador passa de W% da renda de 12 meses? | 25% |
| `brick_reit` | Se há FII, algum é de tijolo? | sem limite |

Os dados são os de `income_events` (o que foi **recebido**, líquido, de
qualquer origem), os mesmos da tela Renda. As janelas de 12 meses são as da
Renda: os 12 meses de calendário que terminam no mês de hoje.

- **`min_yield`**: renda líquida recebida em 12 meses ÷ **valor de hoje** das
  ações, FIIs, REITs e ações de fora (as classes que pagam provento; renda fixa
  e cripto ficam fora do denominador). É uma aproximação: usa a carteira de hoje
  sobre a renda de ontem. Precisa de **12 meses** de proventos registrados.
- **`min_paying_share`**: dos ativos com valor nessas mesmas classes, quantos
  tiveram provento recebido (> 0) em 12 meses. A resposta nomeia quem não pagou.
  Ativo comprado há pouco ainda pode não ter pago: a tela avisa. Precisa de 12
  meses de proventos.
- **`income_growth`**: renda dos 12 meses de agora **maior** que a dos 12
  anteriores (igual não é maior). Precisa de **24 meses** de histórico, para as
  duas janelas estarem inteiras; com uma janela pela metade a conta daria um
  "cresceu" falso. Sem renda nenhuma nas duas, sem dado.
- **`income_every_month`** e **`max_payer`**: as da Saúde, com as regras de
  histórico de lá (3 meses; `max_payer` pede 2 pagadores).
- **`brick_reit`**: FII de **tijolo** tem renda ligada ao aluguel, que costuma
  acompanhar a inflação. Ok se há pelo menos um; sem FII, ou com FII sem o tipo
  conhecido (fonte da CVM), sem dado.

### 3. Rentabilidade

| Regra | Pergunta | Limite de fábrica |
|---|---|---|
| `beat_cdi_12m`, `beat_cdi_5y` | A carteira rende ao menos o CDI? | sem limite |
| `beat_ibov_12m`, `beat_ibov_5y` | …o Ibovespa? | sem limite |
| `beat_ipca_12m`, `beat_ipca_5y` | …o IPCA? | sem limite |
| `min_positive_years` | Pelo menos X% dos anos civis fechados no azul? | 70% |
| `max_volatility` | A volatilidade anualizada é de até Y%? | 20% |
| `max_loss` | Nenhum ativo perdeu mais que Z% desde a compra? | 30% |

A rentabilidade é a **por cotas** do Desempenho ([desempenho.md](desempenho.md)):
aporte não conta como ganho.

- **Contra índice** (`beat_*`): compara a carteira e o índice **na mesma
  janela**. O fim da janela é o menor entre hoje e o último dado publicado do
  índice (o IPCA sai com atraso: comparar a carteira até hoje com um IPCA que
  acabou há dois meses favoreceria a carteira), e o começo é 12 ou 60 meses
  antes do fim. Ok se a rentabilidade arredondada em duas casas é **maior ou
  igual** à do índice. O número de agora é o da carteira e o "limite" é o do
  índice. **Sem dado** se a série da carteira não cobre a janela inteira (a cota
  de base tem de ser do início da janela ou de antes), se a última cota tem mais
  de 7 dias, ou se o índice guardado começa mais de 10 dias depois do início.
- **`min_positive_years`**: anos civis **inteiros e já fechados** (a série tem
  base de antes de 1º de janeiro e vai até dezembro) com retorno composto
  positivo. Precisa de **2** anos assim; o ano em que a série começa e o ano
  corrente não entram.
- **`max_volatility`**: desvio-padrão **amostral** dos retornos mensais dos
  últimos 36 meses, vezes `√12`, em %. O mês em que a série começa e o de hoje
  (parciais) ficam de fora. Precisa de **12** meses de retorno.
- **`max_loss`**: para cada ativo com **custo conhecido** no livro (`cost_basis`),
  `(custo − valor de hoje) ÷ custo`. Ok se a maior perda é de até Z%; o aviso
  lista os que passaram. Ativo sem custo (saldo inicial sem preço) não entra;
  sem nenhum ativo com custo, sem dado. É a regra mais crua: não distingue uma
  perda de capital de uma queda temporária, e por isso não manda vender.

### 4. Qualidade dos ativos

Lê os **fundamentos da CVM** ([fundamentos.md](fundamentos.md): balanços das
companhias e informes dos FIIs). Cada regra pergunta **quanto do valor** de um
grupo de ativos (as empresas, ou os FIIs) passa num critério, no estilo "pelo
menos X% do valor em ações vem de empresas com ROE de 10% ou mais". O limite
editável é o X (percentual, de fábrica **70%**); o critério é fixo e está na
tabela. A conta pesa só quem **tem o dado**: banco numa medida que o plano de
contas dele não tem (dívida, EBITDA, receita), ano sem balanço, fundo de papel
na vacância ficam **fora** da conta, nunca viram reprovados. O número de agora
é o percentual do valor que passa e o aviso diz quantos ativos entraram na
conta ("1 de 2"), quem reprovou e em quem a medida **não se aplica** (banco,
seguradora, holding), com o motivo.

| Regra (`rule`) | Passa quando | Grupo |
|---|---|---|
| `quality_roe` | ROE dos últimos 12 meses ≥ 10% (lucro dos controladores ÷ patrimônio deles) | empresas |
| `quality_profit` | lucro positivo em ≥ 4 dos 5 exercícios (precisa de 4 conhecidos) | empresas |
| `quality_leverage` | dívida líquida ÷ EBITDA ≤ 3 vezes (caixa líquido passa) | empresas, **sem bancos, seguradoras e holdings** |
| `quality_margin` | margem EBITDA > 0 | empresas, sem bancos, seguradoras e holdings |
| `quality_cash` | média do caixa operacional dos 4 últimos exercícios > 0 (os 4 conhecidos) | empresas, sem bancos e seguradoras |
| `quality_recovery` | nenhuma empresa em recuperação judicial (cadastro da CVM) | empresas; **sem limite** |
| `quality_fii_pvp` | P/VP ≤ 1,10 (cotação do Prumo ÷ valor patrimonial por cota) | FIIs |
| `quality_fii_vacancy` | vacância física ≤ 15% (informe trimestral) | FIIs de tijolo; o de papel fica sem dado |

- **Empresa** é ação de Nacionais com balanço na CVM; **FII** é o fundo com
  informe mensal. ETF, BDR, ações e REITs dos EUA, cripto e renda fixa não têm
  fundamentos e ficam fora (`Unsupported` em `fundamentos.md`).
- O EBITDA do Prumo é uma **aproximação** (resultado operacional + depreciação
  e amortização), como em `fundamentos.md`. Nos 12 meses vale o último
  exercício mais o ITR, quando há.
- **Banco e seguradora**: dívida, EBITDA e receita vêm como `financial_template`
  (não se aplica): nem ok, nem falha, e fora do denominador.
- **Holding de participações** (`holding` nos fundamentos: setor de
  participações no cadastro da CVM e a equivalência patrimonial com pelo menos
  metade do lucro, ver [resultados.md](resultados.md#não-se-aplica-holding-banco-e-seguradora)):
  o EBITDA leva o lucro das participadas, então dívida ÷ EBITDA e margem EBITDA
  não se aplicam; ROE, lucro e caixa operacional valem.
- O detalhe de cada regra (`detail.not_applicable`) lista, do maior valor para
  o menor, os ativos do grupo em que a medida não se aplica e por quê (`bank`,
  `insurer`, `holding`); a tela escreve "Não se aplica a ITSA4 (holding)".
- `quality_recovery` não pesa por valor: é a contagem de empresas em
  recuperação judicial e só passa com zero. Sem nenhuma empresa com cadastro, sem
  dado.
- **Sem dado em nenhum ativo do grupo**, a regra fica `no_data`; sem fundamentos
  nenhum (a busca ainda não rodou), as oito ficam `no_data` e a dimensão não
  tem pontuação ("Nenhuma regra com dado ainda").

#### Pontos de atenção dos ativos

Avisos curtos e factuais, por ativo (do maior valor para o menor), sem
recomendação. Só aparece o que passou do corte; o limite exato **não** é ponto:

| Ponto (`kind`) | Quando | Dado |
|---|---|---|
| `income_drop` | a renda de 12 meses do ativo caiu **10% ou mais** contra os 12 anteriores | `income_events` (recebido, líquido); precisa de 24 meses de histórico |
| `low_revenue_growth` | CAGR da receita em 5 anos < 5% ao ano (não vale para banco, seguradora nem holding) | fundamentos |
| `low_earnings_growth` | CAGR do lucro em 5 anos < 5% ao ano | fundamentos |
| `low_roe` | ROE < 10% | fundamentos |
| `low_net_margin` | margem líquida < 10% (não vale para banco, seguradora nem holding: a conta é outra) | fundamentos |
| `debt_above_equity` | dívida líquida > patrimônio líquido (mais de 1 vez) | fundamentos |
| `judicial_recovery` | em recuperação judicial | cadastro da CVM |

CAGR sem número (prejuízo numa das pontas, ano faltando) não vira ponto: o motivo
fica nos fundamentos. A resposta traz também `assessed` (ativos com
fundamentos) e `without_data` (ações e FIIs ainda sem dados): sem nenhum ativo
avaliado a tela diz "ainda sem dados de fundamentos", nunca "tudo certo".

## Perfil de risco

A barra entre **renda fixa** (Renda Fixa e Renda Fixa Internacional) e **renda
variável** (todo o resto: ações, FIIs, REITs, exterior, cripto), com o rótulo
que a fatia de renda variável merece. Os limiares são do Prumo:

| Renda variável | Rótulo |
|---|---|
| até 30% | Conservador |
| de 30% (exclusive) a 70% | Moderado |
| acima de 70% | Arrojado |

A tela mostra o perfil **de agora** (valor de mercado) e o perfil **das metas
por classe** da pessoa (os pontos-base de cada classe), uma barra sob a outra no cartão do veredito, com a
diferença em pontos percentuais. Sem ativos ou sem metas, diz o que falta. As
duas misturas somam 100%.

## Destaques de 12 meses

Quem mais subiu e quem mais caiu (até 3 de cada), por duas medidas:

- **Variação do preço em 12 meses**: o preço de hoje (em reais) contra o último
  fechamento até a data de 12 meses atrás (`price_history`; de até 10 dias antes,
  senão o ativo fica de fora). **Ajustado por desdobramento, grupamento e
  bonificação** do livro: o preço guardado é o do dia, sem ajuste, e comparar 40
  de antes do desdobramento com 22 de hoje pareceria uma queda de 45%. O fator é
  o produto das razões de quantidade desses lançamentos **depois** da data-base.
  Ativo sem 12 meses de histórico de preço, sem preço ou com preço manual não
  entra.
- **Rentabilidade desde a compra**: preço de hoje contra o preço médio do livro
  (`return_percent`). Ativo sem custo conhecido não entra.

Só entra na lista de cima quem subiu e na de baixo quem caiu. É um fato do
passado: não diz comprar nem vender.

## Referências de preço

Opcional e pequeno. **Referência pública, não é recomendação**: fórmulas abertas
sobre dividendos, balanços e taxas de mercado, sem dizer se o ativo está barato
ou caro. As ações têm Bazin e Graham; os FIIs, VP e Rendimento (Bazin e Graham
**não valem** para FII: fundo não tem LPA, e a média de 5 anos de proventos de um
FII esconde a troca de juro que move o preço dele).

### Ações: Bazin e Graham

- **Bazin**: `média anual de dividendos por cota ÷ rendimento-alvo`. O
  rendimento-alvo é da pessoa (`bazin_yield`, padrão **6% ao ano**, de 0
  exclusive a 100). A média é sobre janelas de 12 meses que terminam hoje, hoje −
  12 meses, e assim por diante, **até 5 anos**. A janela de agora entra sempre; as
  mais velhas só entram **inteiras**, ou seja, se o primeiro dividendo conhecido é
  de antes do início delas — uma janela em que o papel ainda nem existia (ou o
  histórico não chega) não vira "zero dividendo". A tela diz quantos anos entraram
  (no ⓘ do Bazin). Dividendo é o bruto por cota (data ex) do cache do Yahoo
  (`dividend_history`), que pede **10 anos** (`dividends.rs`; o cache se refaz em
  um dia): com 5, o primeiro dividendo do histórico era sempre depois do começo da
  quinta janela, e a média nunca passava de 4 anos.
- **Graham**: `√(22,5 × LPA × VPA)`, com lucro e patrimônio **por ação**: o LPA é
  o lucro dos controladores dos últimos 12 meses ÷ ações e o VPA, o patrimônio
  deles ÷ ações (balanços da CVM e número de ações da B3, `fundamentos.md`). Só
  tem valor com os **dois positivos**; prejuízo (`loss`) ou patrimônio negativo
  (`negative_equity`) zeram a raiz.
- **Pelas duas**: o **menor** valor entre Bazin e Graham; com uma referência só,
  é ela. Abaixo dele, o preço passa nos dois critérios (dividendos e lucro com
  patrimônio). Não é a média: a média deixaria passar um preço acima de uma das
  duas.

### FIIs: VP e Rendimento, pelo tipo do fundo

- **VP**: o valor patrimonial por cota (patrimônio líquido ÷ cotas emitidas do
  informe mensal da CVM, `fundamentos.md`): o preço de **P/VP 1**. Vale para todo
  tipo. Patrimônio zero ou negativo: sem VP (`negative_equity`).
- **Rendimento**: `proventos por cota dos últimos 12 meses ÷ rendimento-alvo do
  tipo`. É o preço em que o que o fundo pagou em 12 meses renderia o alvo.
  - **Proventos de 12 meses**: a **mesma conta** da estimativa de renda
    (`income::estimate_assets`): anúncios da B3 com o Yahoo de reserva, janela de
    365 dias pela data ex e a base de cotas de hoje (`proventos-automaticos.md`
    § 9). O provento de FII é isento para pessoa física, então bruto e líquido
    são o mesmo número.
  - **Tipo**: o que o Prumo já deriva do balanço do informe mensal da CVM
    (`sectors::cvm`, corte de dois terços): tijolo, papel, híbrido ou fundo de
    fundos. A **composição** (frações de imóveis, recebíveis e cotas de FII, que
    somam 1) passou a ser guardada junto do tipo, em `sector_reference`.
  - **Alvo por tipo**:

    | Tipo | Alvo | Por quê |
    |---|---|---|
    | Tijolo | juro real do Tesouro IPCA+ de ~10 anos + **prêmio do tijolo** (padrão **2,5 p.p.**) | o aluguel do imóvel é reajustado pela inflação: o provento do tijolo compete com um título que paga IPCA + juro real. O prêmio paga o risco do imóvel (vacância, inadimplência, iliquidez) acima do título público |
    | Papel | CDI de 12 meses × (1 − 15% de IR) + **prêmio do papel** (padrão **1,0 p.p.**) | o CRI rende CDI (ou IPCA) mais spread; o provento do FII é isento e o CDI paga IR (15% na menor alíquota), então a comparação é com o CDI líquido. O prêmio paga o risco de crédito dos recebíveis |
    | Híbrido | `peso dos imóveis × alvo do tijolo + peso dos recebíveis × alvo do papel` | a carteira é parte de cada um; as **cotas de FII contam metade para cada lado** (a carteira de baixo não é conhecida) |
    | Fundo de fundos | a **média simples** dos alvos de tijolo e papel | os fundos de baixo podem ser de qualquer tipo e não são conhecidos; a tela diz isso no ⓘ |

    Os alvos e as contas intermediárias têm duas casas, com o meio para longe do
    zero (11,345% → 11,35%), como a tela mostra.
  - Exemplo fictício (juro real 7,12%, CDI 12m 14,20%): tijolo `7,12% + 2,5 p.p.
    = 9,62%`, e R$ 13,20 de proventos em 12 meses dão `R$ 13,20 ÷ 9,62% =
    R$ 137,21`; papel `14,20% × 0,85 = 12,07% + 1,0 p.p. = 13,07%`; híbrido com
    50% em imóveis, 30% em recebíveis e 20% em cotas → 60% tijolo e 40% papel,
    `0,6 × 9,62% + 0,4 × 13,07% = 11,00%`; fundo de fundos `(9,62% + 13,07%) ÷ 2
    = 11,35%`.
  - Sem Rendimento, com o motivo: tipo desconhecido (`unknown_fund_type`: o fundo
    não está no informe), híbrido sem composição (`no_composition`), sem a taxa
    do alvo (`no_market_rate`), alvo não positivo (`non_positive_target`, juro
    real negativo com prêmio zero) ou sem provento em 12 meses (`no_dividends`).
- **Pelas duas**: o menor de VP e Rendimento; com um só, é ele.
- **Prêmios editáveis** em Ajustes › Análise, junto do rendimento-alvo do Bazin:
  de 0 a 20 p.p., com até duas casas (`brick_premium`, `paper_premium` em
  `analysis_settings`; nulo = o padrão). A tela mostra o juro real e o CDI de hoje
  com a data e o alvo que sai de cada prêmio.

### As taxas de mercado

- **Juro real**: a **taxa de compra** (`Taxa Compra Manha`) do título IPCA+ do
  Tesouro Direto que vence **mais perto de 10 anos** depois da data-base, com a
  **NTN-B** ("Tesouro IPCA+ com Juros Semestrais", o título que o mercado usa
  como "juro real de 10 anos") na frente da NTN-B Principal. Sai do mesmo CSV do
  Tesouro Transparente que a rodada diária já lê em fluxo
  ([`tesouro.md`](tesouro.md#juro-real-de-referência)); fica guardada com o
  título e a data-base (`treasury_real_rate`). Ex.: "juro real 7,60% em
  26/11/2026 (Tesouro IPCA+ com Juros Semestrais 2037)".
- **CDI de 12 meses**: o acumulado das taxas diárias do CDI (BCB/SGS 12) nos 12
  meses que terminam na última taxa publicada, do `index_history` que o
  Desempenho já guarda. A tarefa diária passou a guardar **sempre** pelo menos
  400 dias de CDI, mesmo sem foto da carteira (antes começava 10 dias antes da
  primeira foto). Série que não cobre os 12 meses (com 7 dias de folga no começo)
  não vira CDI.

### Uma conta só, e o teto

- **Uma função só**: a Análise, a escolha do teto, os chips do plano para ativo
  sem teto, o teto do plano (o motor), a carteira e os alertas leem as
  referências por `crate::ceiling` (que só lê o banco) sobre as regras puras de
  `domain::analysis::references` (ações) e `domain::analysis::fund_references`
  (FIIs), e o teto de hoje sai de `domain::ceiling::effective`.
- **O teto é escolha da pessoa**: nenhum, um **método** (Bazin, Graham ou Pelas
  duas nas ações; VP, Rendimento ou Pelas duas nos FIIs) ou um **valor próprio**.
  Com método, grava-se o método e o valor recalcula sozinho quando mudam o
  balanço, os proventos ou as taxas; método sem valor tira o ativo do aporte com
  o motivo ([`alocacao.md`](alocacao.md#teto-de-preço-por-ativo-ceiling)).
- **Na tela**: duas tabelas, Ações (Bazin, Graham, Pelas duas) e FIIs (Tipo, VP,
  Rendimento, Pelas duas), cada uma com "Seu teto" (valor e método, ou "Sem
  teto") e "Hoje". "Teto ▾" abre a escolha — Sem teto, cada método com o valor
  de hoje e o chip aprova/não aprova contra o preço de hoje, Valor próprio — e
  confirmar grava pelo `PATCH /assets/{id}`. O Prumo não calcula "margem" nem
  "potencial".

## Configuração e migração

- As regras da pessoa ficam em `health_rules` (só o que ela mexeu; o resto vale
  o padrão do código). A **migração 0021** apenas amplia o que a tabela aceita:
  as linhas que existem ficam exatamente como estão, e quem nunca mexeu nas
  regras novas as vê ligadas, com o limite de fábrica. O rendimento-alvo do
  Bazin mora na tabela nova `analysis_settings` (só existe linha para quem
  mexeu).
- A **migração 0022** só amplia o `check` de `health_rules` com as oito regras de
  Qualidade (linhas existentes intactas); quem nunca mexeu nelas as vê ligadas, com
  70%.
- A **migração 0024** guarda a escolha do teto em `assets.ceiling_method` (o
  valor próprio continua em `max_price`, então quem tinha teto fica com ele como
  valor próprio) e os prêmios dos FIIs em `analysis_settings` (o `bazin_yield`
  passa a aceitar nulo = 6%). A **0025** cria `treasury_real_rate` e acrescenta a
  composição a `sector_reference`. Nenhuma das duas zera marca para disparar
  trabalho: o código novo relê a CVM enquanto houver FII sem composição guardada
  e baixa a tabela do Tesouro enquanto não houver juro real guardado (o dado é a
  condição, então um pod antigo rodando durante o deploy canário não "consome"
  a releitura).
- `GET`/`PUT /analysis/rules` leem e gravam **a mesma configuração** que
  `GET`/`PUT /health/rules`: as sete originais aparecem nos dois, e as novas só
  em `/analysis/rules`. Os endereços antigos continuam respondendo (o motor de
  avisos e os alertas do Telegram dependem deles).
- Na tela, a configuração é a seção **Ajustes, Análise** (`#/ajustes/analise`). O
  endereço antigo `#/ajustes/saude` continua abrindo e troca o endereço pelo novo
  sem entrada nova no histórico.

## Limites conhecidos

- O `min_yield` e o `min_paying_share` não sabem **quando** cada ativo foi
  comprado: usam a carteira de hoje contra a renda de 12 meses.
- O `max_volatility` usa retornos **mensais** (não diários): é mais estável com a
  série que o Prumo guarda, e subestima picos de um dia.
- O `max_loss` olha o custo do livro: ativo com saldo inicial sem preço não
  entra. Em renda fixa por valor, o "valor" já inclui o rendimento que a pessoa
  informou.
- Os cortes da Qualidade (ROE 10%, 4 de 5 anos, 3 vezes o EBITDA, P/VP 1,10,
  vacância 15%) e dos pontos de atenção são do Prumo e fixos; só o percentual do
  valor (70%) é editável. O balanço atrasa: a DFP sai até março e o ITR até 45
  dias depois do trimestre.
- Empresa sem o total de ações da B3 fica sem LPA e VPA e, portanto, sem Graham.
- O 12 meses de preço de um ativo com preço manual ou renda fixa por valor não
  existe (não há `price_history`).
- Os percentuais seguem o preço ao vivo: um ponto a ver pode aparecer e sumir ao
  longo do dia perto do limite.
- O texto do Telegram dos alertas ainda diz "Saúde da carteira" para o aviso novo
  das sete regras originais.

## Operação

- Nenhum host externo novo: a Análise lê só o que já está guardado. O pedido de
  dividendos ao Yahoo passou de `range=2y` para `range=5y` (mesmo host).
- Tabelas: `health_rules` (regras), `analysis_settings` (Bazin e prêmios),
  `treasury_real_rate` (juro real) e a composição em `sector_reference`. Os
  fundamentos vêm de `Store::fundamentals_for_user` (`fundamentos.md`), lidos a
  cada `GET /analysis`. Nenhum host externo novo: o juro real sai do CSV do
  Tesouro que já é baixado, e o CDI, do SGS que o Desempenho já consulta.
