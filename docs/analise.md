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

**Aguardando a frente de fundamentos** (dados abertos da CVM: retorno sobre o
patrimônio, margens, dívida, crescimento do lucro em 5 anos, recuperação
judicial, P/VP de FII). Até lá, a dimensão **não tem regra**, não entra na
pontuação e a tela diz "Aguardando dados de fundamentos" — o mesmo para os
**pontos de atenção dos ativos**. Nunca aparece "tudo certo".

**Onde se liga** (para quem integrar os fundamentos):

1. `domain/analysis.rs`: acrescentar as `RuleId` da dimensão (o `dimension()` de
   cada uma devolve `Dimension::Quality`), com limite padrão e validação, e
   avaliá-las em `evaluate`/`measure` a partir de um campo novo em `Input`
   (por ativo: ROE, margens, dívida, CAGR do lucro, recuperação judicial, P/VP).
   Enquanto `dimensions()` não achar regra da dimensão, ela é "aguardando"; com
   regra, vira "pronta" e ganha pontuação sozinha.
2. `migrations`: a tabela `health_rules` ganha as regras novas no `check`
   (como a 0021 fez); nada mais.
3. `analysis.rs` (junção): `report(.., fundamentals: &Fundamentals)` já recebe a
   estrutura `Fundamentals { available, per_share }`; o handler
   (`http/analysis.rs`) hoje passa `Fundamentals::awaiting()` — é a linha que
   passa a ler do banco. `available: true` liga o aviso do Graham.
4. `frontend`: `RULE_TEXT`/`NO_DATA_TEXT` (em `lib/analysisText.ts`) e
   `ANALYSIS_RULE_IDS` (em `api/analysisTypes.ts`) ganham as regras; a tela
   `DimensionCard` já mostra a dimensão pronta, e `AssetAttention` (em
   `AnalysisScreen.tsx`) passa a listar os pontos de atenção.

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
por classe** da pessoa (os pontos-base de cada classe), lado a lado, com a
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
sobre o histórico de dividendos, sem dizer se o ativo está barato ou caro.

- **Bazin**: `média anual de dividendos por cota ÷ rendimento-alvo`. O
  rendimento-alvo é da pessoa (`bazin_yield`, padrão **6% ao ano**, de 0
  exclusive a 100). A média é sobre janelas de 12 meses que terminam hoje, hoje −
  12 meses, e assim por diante, **até 5 anos**. A janela de agora entra sempre; as
  mais velhas só entram **inteiras**, ou seja, se o primeiro dividendo conhecido é
  de antes do início delas — uma janela em que o papel ainda nem existia (ou o
  histórico não chega) não vira "zero dividendo". A tela diz quantos anos entraram.
  Dividendo é o bruto por cota (data ex) do cache do Yahoo (`dividend_history`),
  que passou a pedir **5 anos** em vez de 2 (`dividends.rs`; o cache se refaz em
  um dia). Só ações e FIIs da B3 (em reais).
- **Graham**: `√(22,5 × LPA × VPA)`, com lucro e patrimônio **por ação**. Precisa
  dos fundamentos da CVM; **sem eles não aparece** ("aguardando dados de
  fundamentos"). Com lucro ou patrimônio zero ou negativo, a raiz não existe e
  nada é mostrado.
- **Teto de preço**: a tela mostra a referência ao lado do teto que a pessoa pôs
  no ativo (`max_price`) e do preço de hoje. Um botão ("Usar Bazin como meu
  teto") aplica o valor como teto **depois de uma confirmação**, pelo mesmo
  `PATCH /assets/{id}` do resto do app. O Prumo não calcula "margem" nem
  "potencial".

## Configuração e migração

- As regras da pessoa ficam em `health_rules` (só o que ela mexeu; o resto vale
  o padrão do código). A **migração 0021** apenas amplia o que a tabela aceita:
  as linhas que existem ficam exatamente como estão, e quem nunca mexeu nas
  regras novas as vê ligadas, com o limite de fábrica. O rendimento-alvo do
  Bazin mora na tabela nova `analysis_settings` (só existe linha para quem
  mexeu).
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
- O 12 meses de preço de um ativo com preço manual ou renda fixa por valor não
  existe (não há `price_history`).
- Os percentuais seguem o preço ao vivo: um ponto a ver pode aparecer e sumir ao
  longo do dia perto do limite.
- O texto do Telegram dos alertas ainda diz "Saúde da carteira" para o aviso novo
  das sete regras originais.

## Operação

- Nenhum host externo novo: a Análise lê só o que já está guardado. O pedido de
  dividendos ao Yahoo passou de `range=2y` para `range=5y` (mesmo host).
- Tabelas: `health_rules` (regras), `analysis_settings` (Bazin).
