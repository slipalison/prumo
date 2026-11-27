# Risco: volatilidade, queda máxima, correlação e risco × retorno

A tela `#/desempenho/risco` (link no cabeçalho do Desempenho) responde "quanto
a minha carteira e cada ativo oscilaram, qual foi a maior queda e quanto os
ativos andam juntos". É **retrato do passado** na janela escolhida: não é
previsão, não diz o que comprar nem vender, e não classifica ativo nenhum como
melhor ou pior. O destaque dos pares que mais andam juntos é neutro: diz o
número, não o que fazer com ele.

Contrato em [`api.md`](api.md) (`GET /api/v1/risk`), desenho em
[`design.md`](design.md) (Desempenho › Risco). Código: `domain/risk.rs` (regra
pura), `risk.rs` (o que lê do banco), `http/risk.rs`, `features/risk/`.

## As séries

Tudo parte de uma série de **nível** por data; o retorno de um pregão é o nível
dele sobre o do pregão anterior, menos 1.

| Série | De onde vem | Nível |
|---|---|---|
| Carteira | `portfolio_snapshots`, a cota do método de cotas ([`desempenho.md`](desempenho.md)) | valor da cota: o aporte não mexe nela, o provento recebido entra como retorno |
| Ibovespa | `index_history` (`ibov`, Yahoo `^BVSP`) | pontos |
| Ativo | `price_history` (fechamento **real** do dia) + `price_splits` + `dividend_history` + câmbio | índice de retorno total em reais (abaixo) |

**O ativo, passo a passo** (as mesmas contas do comparador,
`domain::compare::adjust_for_splits` e `total_return_levels`):

1. **Pé de hoje.** `price_history` guarda o que se pagava no dia; num
   desdobramento de 1 para 2 o fechamento cai à metade, o que seria −50% num
   pregão. Cada fechamento é dividido pelo produto dos fatores dos
   desdobramentos e bonificações **depois** dele (`price_splits`, migração
   0030, os eventos que o Yahoo informa).
2. **Proventos dentro.** O índice começa no primeiro fechamento e anda
   `(A_t + D_t) / A_{t−1}`, com `D_t` a soma dos proventos por cota com data ex
   depois do fechamento anterior e até `t` (`dividend_history`, do Yahoo: bruto,
   no mesmo pé do preço ajustado). É o que rende quem compra a cota e guarda o
   provento: sem isso, um FII que distribui 10% ao ano e fica com a cota parada
   apareceria com retorno zero, e a data ex de cada mês, como uma queda.
3. **Em reais.** Ativo dos EUA e cripto: o nível em dólar vezes o câmbio
   `USDBRL=X` do mesmo dia. A volatilidade e a correlação são as de quem tem o
   ativo em reais (o câmbio entra).

**Títulos do Tesouro.** O preço do Tesouro (CSV do Tesouro Transparente) do
título sem cupom (Selic, Prefixado, IPCA+) entra como qualquer ativo. O que
paga no caminho — juros semestrais (Prefixado e IPCA+ com juros, IGPM+) e a
renda mensal do Renda+ e do Educa+ — fica **fora das contas por ativo**: a
tabela traz o preço que cai a cada pagamento, sem o pagamento, e a queda
pareceria perda (`domain::risk::pays_before_maturity`). Na cota da carteira o
pagamento entra como provento, se estiver lançado.

**Sem série de preço** (renda fixa por valor, preço manual): aparece na tabela
com o peso e o motivo, sem número.

## Pregões e alinhamento

As datas de pregão são as datas do Ibovespa mais as de qualquer fechamento de
ativo da B3 guardado (as duas fontes só têm dia de pregão; juntas, a falha de
uma não abre buraco). Sem nenhuma das duas (carteira só de exterior e cripto, e
o índice fora do ar), valem os dias úteis de segunda a sexta.

Cada série é posta nessas datas pelo **último valor em ou antes** de cada uma,
desde que de no máximo **10 dias corridos** antes (`MAX_STALE_DAYS`): o feriado
americano em dia de pregão aqui repete o fechamento de lá (retorno zero naquele
dia, e o movimento entra no dia seguinte); a cripto, que negocia no fim de
semana, entra na segunda com o movimento do sábado e do domingo. Depois de 10
dias sem fechamento a série para (ativo deslistado não vira uma reta de retorno
zero) e volta quando volta o preço; retorno só existe entre dois pregões
seguidos com valor.

## A janela

`6m`, `12m` (padrão), `24m`, `36m` e `60m`. O **fechamento de base** é o último
pregão em ou antes da mesma data, N meses atrás; os retornos vão dele até o
último pregão guardado. Uma série que começa depois da base (ativo lançado há
pouco, carteira mais nova que a janela) usa o que tem e vem com
`full_window: false` e a data em que começa.

## Fórmulas

Com `r_t` os retornos diários da janela, `n` quantos são:

- **Volatilidade anualizada** = desvio-padrão amostral dos `r_t` (divisor
  `n − 1`) × √252. 252 é a convenção de pregões por ano (a B3 tem de 246 a
  252). Retornos simples, não logarítmicos.
- **Retorno no período** = último nível ÷ nível de base − 1 (acumulado, sem
  anualizar: em 6 meses é o de 6 meses). Na carteira, pelo mesmo método de
  cotas da tela de Desempenho, medido do fechamento de base (um pregão).
- **Queda máxima** = o menor `nível / pico anterior − 1` na janela, com o
  **pico** (de onde a queda começou), o **fundo** (o ponto mais baixo) e a
  **recuperação** (o primeiro dia depois do fundo em que o nível voltou ao do
  pico) ou "ainda não recuperou". Série que só subiu: "sem queda".
- **Correlação** (de Pearson) de dois ativos = `cov(r_a, r_b) / (σ_a σ_b)`,
  só nos pregões em que **os dois** têm retorno, de −1 (um sobe quando o outro
  cai) a 1 (sobem e caem juntos). Cada par usa todas as datas que tem em comum
  (correlação "par a par").
- **Os pares que mais andam juntos**: os 5 maiores valores fora da diagonal,
  do maior para o menor.
- **Carteira de hoje na janela** (`model`): `R_t = Σ wᵢ rᵢ,t`, com `wᵢ` o peso
  de hoje de cada ativo na carteira inteira, em cada pregão em que **todos** os
  ativos da conta têm retorno; `σ_p` é a volatilidade anualizada de `R`. O que
  não está na conta (renda fixa por valor, ativo sem série) entra como se não
  oscilasse: o peso dele dilui `σ_p`.
- **Parte do risco** de cada ativo (decomposição de Euler):
  `wᵢ · cov(rᵢ, R) / var(R)`. As partes somam 100%; uma parte negativa quer
  dizer que o ativo andou contra o resto na janela e tirou oscilação.

**Quem entra em quê:**

| | Volatilidade, retorno, queda | Correlação | Parte do risco |
|---|---|---|---|
| Ativo com ≥ 20 retornos na janela | sim | só com ≥ 60 | só com ≥ 60 **e** cobrindo a janela inteira |
| Ativo com < 20 retornos ("histórico curto") | não | não | não |
| Título que paga no caminho, ativo sem série | não | não | não (conta como risco zero) |

`MIN_RETURNS = 20`: com menos, o desvio-padrão erra demais (≈ 16% para mais ou
para menos com 20 pontos). `MIN_COMMON_RETURNS = 60` (uns 3 meses de pregões):
com 60 pontos, dois ativos sem relação nenhuma ainda mostram correlação de até
±0,13 por acaso; com menos, o número diria pouco. A matriz não mostra o par com
menos de 60 pregões em comum (`—`).

### Conferência à mão (dado fictício)

Dois ativos sem relação entre si (correlação 0), os dois com volatilidade de
16% ao ano, pesos de 30% e 10%; o resto da carteira (60%) é renda fixa por
valor.

- `σ_p = √(0,3² · 16%² + 0,1² · 16%²) = √0,10 · 16% ≈ 5,06%` ao ano.
- Parte do primeiro: `0,3² · 16%² / (0,10 · 16%²) = 90%`; do segundo, 10%.

O teste `risk_shares_by_hand` (`domain/risk.rs`) faz essa conta com retornos de
±1% ao dia; `risk_route_end_to_end` (`tests/risk.rs`) refaz a volatilidade, o
retorno e a queda do Ibovespa, de um ativo que desdobra, de um FII que paga
provento e de um ativo em dólar, à mão, sobre séries gravadas no banco.

## O histórico precisa existir: a sincronia

O risco não busca nada na hora da tela. Quem enche o banco é a sincronia de
preços do Desempenho (`performance.rs`), em segundo plano, com o advisory lock
de sempre:

- **5 anos de preço** para todo ativo com livro: o histórico começa no
  lançamento mais antigo **ou** em `domain::risk::history_start` (a janela de
  60 meses mais 10 dias de folga para a base), o que vier antes — antes, ele
  começava só no primeiro lançamento, e um ativo comprado há dois meses não
  tinha como ter volatilidade de 12 meses. O câmbio da moeda do ativo vem
  junto. O título do Tesouro que paga no caminho fica com o histórico desde o
  primeiro lançamento (não entra no risco).
- **O Ibovespa** também desde `history_start` (os outros índices continuam
  desde a primeira foto).
- **Os desdobramentos** vêm no mesmo pedido do Yahoo (`events=split`, até
  hoje) e vão para `price_splits`, com a conferência em `price_split_checks`
  (as tabelas do comparador, migração 0030). Símbolo guardado antes disso, sem
  conferência desde o começo do histórico, é pedido de novo uma vez, de ponta a
  ponta; o fechamento que já estava guardado não muda (`on conflict do
  nothing`) e, por isso, não suja a série de ninguém (`insert_prices` devolve só
  as datas que entraram de fato).
- **Logo depois do deploy**: a migração 0032 marca como "suja" (desde a última
  foto) a série de quem já tem uma; a reconstrução em segundo plano, que olha as
  marcas a cada 10 s, roda a sincronia na hora, sem esperar a tarefa das 19h. O
  preço antigo que chega marca, como sempre, a série de quem tem o ativo, que é
  refeita uma vez.

Custo: um pedido ao Yahoo por símbolo para trás (uns 1.250 fechamentos, ~60 kB)
uma vez só; depois, o pedido diário de sempre. A tabela do Tesouro (~15 MB, em
fluxo) é baixada uma vez para os títulos sem cupom que faltam.

## Limites

- **Fonte única** (Yahoo) para preço, desdobramento e provento por cota: dado
  pode faltar, atrasar ou ser corrigido depois; o cache guarda como estava no
  pedido.
- **Provento do ativo × provento da carteira.** No índice do ativo o provento é
  o bruto do Yahoo, na data ex (JCP bruto incluído); na cota da carteira, é o
  líquido recebido, na data de pagamento. A diferença é pequena na volatilidade
  e aparece um pouco no retorno.
- **Mercados que fecham em horários diferentes** (B3 e EUA, cripto 24 h): a
  correlação entre eles sai um pouco menor do que a real, porque o mesmo
  movimento cai em pregões diferentes.
- **√252 supõe retornos independentes** de um dia para o outro; é a convenção,
  não uma lei.
- **Matriz par a par × conta da carteira de hoje.** A matriz usa, para cada
  par, todas as datas que os dois têm; a conta da carteira de hoje usa só as
  datas em que todos têm. Com históricos iguais (o normal, depois da sincronia
  de 5 anos) dá no mesmo; com um ativo novo, o número da matriz pode diferir um
  pouco do implícito na parte do risco.
- **Pesos de hoje** na parte do risco: a pergunta é "de onde vem a oscilação da
  carteira como ela está agora", não a da carteira que existiu na janela (essa
  é a volatilidade pelas cotas, no topo da tela).
