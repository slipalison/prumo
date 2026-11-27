# Projeção por cenários

Duas contas, as duas **cenário com premissas à vista e editáveis, nunca
promessa** (regra do produto: o Prumo informa, não recomenda; número nunca vem
de LLM):

1. **Objetivos com projeção** (Ajustes › Objetivos): quando o patrimônio, o
   valor numa classe ou a renda mensal chega à meta, em três cenários, com a
   curva projetada e a faixa num gráfico.
2. **Renda projetada** (painel na tela Renda): a renda dos próximos 12 ou 24
   meses, por ativo e por tipo, em três cenários.

Código: a regra pura em `backend/src/domain/projection/` (`rates.rs` o caminho
de juro e inflação, `income.rs` a renda projetada, `goal.rs` a simulação dos
objetivos, `progress.rs` o progresso da meta de renda); o que lê o banco em
`backend/src/projection.rs`; a busca do Focus em `backend/src/focus.rs`; as
rotas em `http/objectives.rs` e `http/projection.rs`. Contrato em
[`api.md`](api.md#objetivos) e [`api.md`](api.md#renda-projetada-get-apiv1incomeprojection).

## Fonte: o Focus do Banco Central

A pesquisa Focus (o Banco Central pergunta ao mercado o que espera de inflação,
juro, câmbio…) é publicada pela API aberta **Olinda**, sem chave nem cadastro:

- `https://olinda.bcb.gov.br/olinda/servico/Expectativas/versao/v1/odata/ExpectativasMercadoAnuais`
  — IPCA no ano e **meta Selic no fim do ano**, do ano de hoje até uns quatro
  à frente;
- `.../ExpectativaMercadoMensais` — IPCA mês a mês, uns 24 meses à frente.

A busca pede só IPCA e Selic, só a mediana, só `baseCalculo = 0` (a janela de
30 dias de respostas que o relatório Focus usa) e 15 dias de pesquisas (cobre
feriado e atraso), e guarda a pesquisa **mais recente** de cada indicador.
Medido em 27/11/2026: duas consultas, uns 18 kB no total; a pesquisa mais nova
era de 19/11/2026. **A Olinda recusa `+` como espaço** no `$filter` (responde
`The types 'Edm.Boolean' and 'Edm.String' are not compatible`), por isso a
consulta vai montada à mão com `%20` (`focus::odata_query`).

- **Frequência:** uma tarefa em segundo plano confere de hora em hora e busca
  quando a última busca tem mais de 20 horas (uma vez por dia). Só com as
  fontes externas ligadas (`QUOTES_ENABLED`, como o resto).
- **Duas réplicas:** quem reivindica a vez em `focus_sync` (um `update`
  condicional, válido por 15 minutos) busca; a outra pula. A busca não segura
  conexão durante a rede; a gravação é uma transação curta com
  `pg_advisory_xact_lock`.
- **Falha não derruba nada:** a busca que falha devolve a vez e não apaga as
  medianas de ontem; a tela mostra a data da pesquisa que está no banco ("Focus
  do Banco Central, pesquisa de 19/11/2026"). Nenhuma requisição da tela espera
  a Olinda.
- **Banco:** `focus_expectations` (indicador, período anual ou mensal, o dia 1º
  da referência, a mediana em %, a data da pesquisa) e `focus_sync` (a última
  busca e a vez), migração `0026_projection.sql`.

O CDI e o IPCA **medidos** vêm da série que o Desempenho já guarda
(`index_history`, do SGS do Banco Central): o CDI de hoje (a taxa diária mais
recente, composta por 252 dias úteis), o CDI médio dos últimos 12 meses e o
IPCA dos últimos 12 meses publicados.

## O caminho de juro e inflação (`rates.rs`)

- **IPCA, mês a mês:** a mediana mensal do Focus; mês sem ela, a anual daquele
  ano em taxa mensal equivalente (`(1 + anual)^(1/12) − 1`); ano sem nenhuma,
  a de longo prazo (abaixo). Sem Focus, o IPCA dos últimos 12 meses medido;
  sem ele, a meta de inflação de 3% ao ano.
- **CDI, mês a mês:** o Focus dá a meta Selic esperada para o **fim** de cada
  ano. O CDI esperado em dezembro do ano `A` é essa meta menos **0,10 ponto**
  (o DI de um dia anda colado na Selic efetiva, que o Banco Central mantém
  0,10 abaixo da meta). Entre um dezembro e outro, o caminho é uma **reta mês
  a mês** que parte do CDI de hoje. Depois do último ano publicado, fica no
  último valor. Sem Focus, o CDI de hoje parado; sem CDI nenhum, a renda de
  papel não se mexe (e a tela diz isso).
- **Inflação de longo prazo** (a premissa padrão dos objetivos): a média
  geométrica das medianas anuais do IPCA dos anos **depois** do de hoje (o ano
  corrente já tem meses publicados). Exemplo fictício: 4,5% em 2027 e 4,0% em
  2028 → `√(1,045 × 1,040) − 1 = 4,25%`. Sem ano à frente, o último publicado.

## Renda projetada (`income.rs`)

**Ponto de partida:** a estimativa da tela Renda — o que cada cota pagou em 12
meses, líquido do IR retido, vezes as cotas de hoje (`crate::income`, a mesma
conta, não uma segunda). O tipo do ativo sai da classe e, no FII, do tipo pelo
balanço da CVM (`sectors/cvm.rs`: dois terços em imóveis = tijolo, em
recebíveis = papel).

| Tipo | Partida (por mês) | Como anda no mês `k` (1 = o mês que vem) |
|---|---|---|
| FII de papel | os 12 meses | × `CDI do mês k ÷ CDI médio dos 12 meses que passaram` |
| FII de tijolo | os 12 meses | × IPCA acumulado do mês 1 ao `k` (o reajuste dos aluguéis) |
| FII híbrido, de fundos ou sem tipo | os 12 meses | parado |
| Ações | a média do habitual (até 5 anos) quando há; senão os 12 meses | parado |
| Exterior (ações, REITs, ETF, BDR) | a média do habitual quando há; senão os 12 meses | parado |

- **Por que o CDI dos 12 meses que passaram:** a renda dos últimos 12 meses do
  FII de papel foi paga com o CDI deles; a de amanhã, com o CDI de amanhã.
  Exemplo fictício: R$ 100 por mês, CDI médio de 14,6% nos últimos 12 meses,
  13,9% esperado no fim do período → no último mês, `100 × 13,9 ÷ 14,6 =
  R$ 95,21`.
- **Por que a média do habitual nas ações:** a soma de 12 meses de uma ação
  carrega o provento extraordinário ("acima do habitual" na Renda); a média de
  até 5 anos é o que ela costuma pagar. Os FIIs pagam todo mês e acompanham o
  juro e o aluguel de agora, então partem dos 12 meses.
- **Calendário:** cada mês recebe o peso que o mesmo mês teve nos últimos 12
  (`Shape`, média 1): pelo dia de pagamento quando a B3 o informa, senão pela
  data ex. A ação que paga em maio e novembro continua pagando em maio e
  novembro; o total do ano não muda.
- **Cenários:** o pessimista e o otimista saem do base por `(1 ∓ amplitude)^(k/12)`
  no mês `k` — a amplitude é uma taxa ao ano (padrão: 2 pontos) e a distância
  entre os cenários cresce com o tempo. Uma regra só para todos os tipos:
  "cada renda cresce a amplitude a menos (ou a mais) por ano que no base".
- **Premissas editáveis na tela:** o CDI e o IPCA do período inteiro (uma taxa
  só, no lugar do Focus) e a amplitude; o horizonte é 12 ou 24 meses. Nada é
  gravado: vai na consulta.

**Limites, escritos:** a conta trata toda a renda do FII de papel como
proporcional ao CDI (CRI atrelado ao IPCA ou com spread alto muda menos); o
reajuste do tijolo é anual por contrato e aqui se espalha mês a mês; vacância,
inadimplência, emissão e venda de imóvel não entram; os proventos já anunciados
não substituem a projeção (estão em "Próximos pagamentos").

## Objetivos com projeção (`goal.rs`)

A simulação é mês a mês, **em reais de hoje** (poder de compra de hoje), com as
taxas anuais convertidas em mensais equivalentes:

```text
valor[m] = valor[m−1] × (1 + taxa)^(1/12) + aporte
  taxa   = real                              (reinvestindo os proventos)
         = (1 + real) / (1 + rendimento) − 1 (sem reinvestir: só o preço)
renda[m] = renda[m−1] × (1 + crescimento)^(1/12)
         + (aporte + (reinveste ? renda[m−1] : 0)) × rendimento / 12
```

- `real` é a rentabilidade real **total** (com os proventos dentro, como a cota
  do Desempenho). Quem retira os proventos todo mês fica só com o preço: a
  total descontado o rendimento.
- O aporte é constante em reais de hoje (sobe com a inflação em reais
  correntes).
- A renda de partida é a estimativa da Renda (da classe, no objetivo de
  classe); cresce pelo crescimento real dos proventos e pelo que o dinheiro
  novo compra de renda ao rendimento líquido da carteira de hoje.

**O que cada objetivo mede:** patrimônio e classe, o valor; a meta de renda, a
renda média do mês (modo `average`) ou o **piso** (modo `floor`): o menor mês
dos 12 seguintes, cada mês com o peso do mesmo mês no ano da carteira (o
calendário da Renda projetada, ponderado pela renda de cada ativo). No piso, a
meta só chega quando o pior mês chega; se a carteira de hoje tem um mês sem
provento nenhum, o piso fica em zero e a resposta diz que não chega.

**Duas leituras da meta, as duas na resposta:**

- **em reais da época** (`nominal`): o número digitado. Em reais de hoje ele
  encolhe com a inflação: `meta ÷ (1 + inflação)^(m/12)`. É a data principal
  (a que a barra de progresso, também nominal, conversa).
- **em reais de hoje** (`today`): o número com o poder de compra de hoje.

A data é o primeiro mês em que o que o objetivo mede chega à meta (até 100
anos). `reached` quando o mês de hoje já chega (na meta de renda, pela
estimativa: a conquista só vale com o recebido); `unreachable` quando a
distância até a meta não diminui em 100 anos (nada faz a conta andar);
`beyond` quando diminui mas não chega.

**Cenários:** o pessimista e o otimista somam `∓ amplitude` à rentabilidade
real e ao crescimento dos proventos (padrão: 2 pontos ao ano). O valor só
depende da primeira e a renda só do segundo, então o otimista nunca chega
depois do base (testado por propriedade). O aporte, a inflação e o rendimento
são os mesmos nos três.

**A curva:** do mês de hoje até a data mais distante entre o base (nas duas
leituras) e o pessimista em reais da época, mais um ano, de 2 a 50 anos (20 sem
data nenhuma), com até ~120 pontos; cada ponto traz os três cenários nas duas
leituras. A tela troca a leitura num botão, e em cada uma a meta é uma linha
reta: o cruzamento é a data daquela leitura.

### As premissas e os padrões

Cada premissa é **editável por objetivo** (nulo = o padrão) e a resposta traz
o valor usado e de onde veio:

| Premissa | Padrão | De onde vem o padrão |
|---|---|---|
| Aporte por mês (reais de hoje) | a média do livro | as compras (`buy`) dos últimos 12 meses ÷ os meses desde a primeira compra (de 1 a 12), da classe no objetivo de classe |
| Rentabilidade real ao ano | a do Desempenho, real | a rentabilidade da carteira em Desempenho (cotas, 12 meses, anualizada, com 6 meses de série pelo menos), descontado o IPCA medido no mesmo período (`(1 + n) ÷ (1 + i) − 1`); sem série, o CDI de hoje menos 15% de IR descontada a inflação padrão; sem nada, 4% ao ano (convenção do Prumo, não previsão) |
| Inflação ao ano | Focus | a inflação de longo prazo do Focus (acima); sem Focus, o IPCA dos últimos 12 meses; sem ele, a meta de 3% |
| Reinvestir os proventos | sim | — |
| Crescimento real dos proventos ao ano | 0% | sem crescimento real: os proventos só acompanham a inflação |
| Rendimento do dinheiro novo ao ano | o da carteira | a renda estimada da Renda × 12 ÷ o valor de hoje (da classe no objetivo de classe; classe vazia, o da carteira) |
| Amplitude dos cenários | 2 pontos ao ano | — |

Faixas aceitas (as mesmas do `check` da tabela): rentabilidade de −50% a 50%,
inflação de −10% a 50%, crescimento de −50% a 50%, rendimento de 0% a 50%,
amplitude de 0 a 20 pontos.

**A premissa de rentabilidade passou a ser real.** Até a migração `0026` a
premissa informada (`expected_return`) era nominal. A migração a converte
descontando o IPCA dos 12 últimos meses que o banco tem (a mesma série do
Desempenho): `real = (1 + nominal) ÷ (1 + ipca_12m) − 1`, e troca a coluna por
`real_return`. Sem os 12 meses de IPCA no banco não há como converter, e a
premissa volta ao padrão (testado com banco nas migrações anteriores e dado
fictício: `tests/projection.rs`).

### Exemplo fictício

Patrimônio de R$ 250.000, meta de R$ 1.000.000, aporte de R$ 2.400 por mês,
rentabilidade real de 5%, inflação de 4%, reinvestindo, amplitude de 2 pontos,
hoje em nov/2026:

| Cenário | Em reais da época | Em reais de hoje |
|---|---|---|
| Otimista (7% real) | nov/2034 (96 meses) | dez/2037 (133) |
| Base (5% real) | out/2035 (107 meses) | jan/2040 (158) |
| Pessimista (3% real) | dez/2036 (121 meses) | fev/2043 (195) |

Meta de renda de R$ 3.000 por mês (média), partindo de R$ 1.000 por mês de
renda estimada, aporte de R$ 2.000, rendimento de 8% ao ano, reinvestindo, sem
crescimento real: base em nov/2031 (60 meses) em reais da época e abr/2033
(77) em reais de hoje; faixa de jul/2031 (otimista, 2% de crescimento real) a
mai/2032 (pessimista, −2%).

### A meta de renda é um objetivo

A meta de renda mensal (`kind = monthly_income`, uma por pessoa) aparece em
`GET /objectives` com a projeção e as premissas e se edita por
`PATCH /objectives/{id}` (o modo `income_mode`, a meta, as premissas); a tela
Renda continua com `/income/goal`, que agora diz o `mode`. O progresso no
**piso** usa o recebido dos 12 meses **fechados** antes do mês de hoje (o mês
corrente ainda tem pagamento por vir); sem nada recebido neles, a estimativa
da Renda × o peso do pior mês do calendário da carteira, marcada como
estimativa. Só o recebido conquista a meta, nos dois modos. Trocar o modo
apaga a data de conquista (o que a meta mede mudou), como trocar o valor.

O impacto do aporte (`POST /objectives/impact`, no Aportar) usa a mesma
simulação (cenário base, meta em reais da época) e cobre só os objetivos de
valor: a renda que o aporte soma já está no plano.
