# Como o Prumo decide o aporte

O código é `backend/src/domain/allocation.rs`; este documento explica o porquê
de cada passo. A rota é `POST /api/v1/plans` ([contrato](api.md)).

## A pergunta

> Tenho `A` reais para investir hoje. O que compro para a carteira ficar o mais
> perto possível das minhas metas?

As **metas** são percentuais por classe (somam 100). A **nota** de cada ativo
(0 a 10, do checklist ou manual) diz quanto da classe aquele ativo deveria
ocupar em relação aos outros da mesma classe.

Dois guarda-corpos opcionais, os dois escolhidos pela própria pessoa, mudam a
conta só quando estão ligados (ver [Teto de preço e limite de
concentração](#teto-de-preço-e-limite-de-concentração)): o **teto de preço**
de cada ativo e o **limite de concentração** por ativo. Desligados, o resultado
é o de sempre, centavo por centavo.

## Os seis passos

### 1. O tamanho da carteira depois do aporte

`N = patrimônio atual + A`. A meta de cada classe em reais é `meta% × N`, e o
**que falta** nela (`gap`) é `max(0, meta − atual)`.

Usar `N` (e não o patrimônio de hoje) é o que faz o aporte de hoje já contar
para a meta de amanhã.

### 2. Quem pode receber

Uma classe é **elegível** quando tem pelo menos um ativo com nota > 0, preço
conhecido e preço até o teto da pessoa (quando ela definiu um) — e, com o
limite de concentração ligado, espaço sob o limite em algum desses ativos.
Classe abaixo da meta e sem ativo elegível fica **bloqueada**, e o aviso diz o
motivo (`reason`): nenhum ativo (`no_assets`), nenhum com nota (`unrated`), os
com nota sem preço (`unpriced`), os com nota e preço acima do teto
(`above_ceiling`) ou os que receberiam já no limite (`concentration_limit`).

O ativo não precisa existir na carteira para ser elegível: cadastrado com
quantidade 0, ele vale 0 hoje, tem nota e preço, e recebe a fatia dele. É assim
que a pessoa monta a lista do que quer ter numa classe ainda vazia — sem isso,
uma carteira concentrada numa classe acima da meta não teria para onde mandar
o aporte, e ele inteiro voltaria como `unallocated`. O motor nunca manda o
dinheiro para a classe que já passou da meta para "não sobrar": isso desfaria
a diversificação que ele existe para manter.

### 3. Entre classes: proporcional ao que falta, com teto no que falta

O aporte é dividido entre as classes elegíveis na proporção do `gap` de cada
uma. Nenhuma classe recebe mais que o próprio `gap`.

Com o limite de concentração ligado, a classe também não recebe mais do que
**cabe** nos ativos elegíveis dela sem nenhum passar do limite (a soma de
`limite − valor atual` de cada um). A classe que bate nesse teto sai da rodada,
e o que ela não comportou é redividido entre as outras elegíveis pelo que ainda
falta em cada uma — de novo com teto no que falta. É a mesma regra de sempre:
uma classe bloqueada já não entra na divisão, e a parte dela vai para as
outras até a meta delas.

Como `Σ metas = N` e `Σ atual = N − A`, vale sempre:

```
Σ gap positivo = A + Σ excesso das classes acima da meta  ≥  A
```

Então o aporte **sempre cabe** no que falta — exceto quando parte do que falta
está numa classe bloqueada, ou numa classe que o limite não deixa encher. Só
nesses casos sobra dinheiro depois de encher as elegíveis, e essa sobra volta
como `unallocated`, com o aviso do motivo: `class_blocked` (qual classe
precisa de um ativo) ou `concentration_limit` (quanto ficou sem destino por
causa do limite).

**Por que não espalhar a sobra entre as outras classes?** Porque isso
empurraria classes elegíveis para cima da meta — exatamente o que a carteira
não quer. Dinheiro sem destino é informação ("cadastre um ativo internacional"),
não algo a esconder.

### 4. Dentro da classe: a nota vira fatia

A fatia ideal de cada ativo é `nota / Σ notas da classe`. Com quatro ativos de
nota 6 e um de nota 2, os de nota 6 deveriam ter 6/26 da classe cada e o de
nota 2, 2/26.

Ativos sem nota, sem preço ou acima do teto continuam ocupando espaço: o valor
deles é descontado antes de dividir a classe entre os elegíveis. O dinheiro vai
para quem está abaixo da própria fatia, na proporção do que falta a cada um.

Com o limite ligado, a fatia de um ativo não passa do limite (ou do que ele já
vale, se já passou: o motor não vende). Quem bate no limite fica nele, e o
resto da classe é redividido **pela nota** entre os outros — o excedente vai
para os outros ativos da classe. Como a classe só recebeu o que cabe (passo
3), a divisão sempre fecha.

**O % ideal que a Carteira mostra** é esta mesma fatia, `nota / Σ notas`, em
percentual: `ideal_class_percents` em `allocation.rs`, e o motor e a tela
passam pela mesma função de fatia. Ativo sem nota ou sem preço tem ideal 0, e
numa classe com algum elegível os ideais somam 100 (conferido por teste de
propriedade). Com `KNRI11` e `HGLG11` de nota 6, `BTLG11` e `TRXF11` de nota 4 e
`MXRF11` de nota 0: 30%, 30%, 20%, 20% e 0%.

**O ideal não muda com o teto de preço.** O ideal é a **composição-alvo** pela
nota: a carteira que a pessoa quer ter. O teto é um **filtro temporário de
compra**: tira o ativo deste aporte enquanto o preço estiver acima dele, sem
tirá-lo da carteira desejada. Por isso um ativo acima do teto mantém o
`ideal_class_percent` dele (e pode até aparecer "abaixo do ideal"), e a
Carteira mostra ao lado o selo "acima do teto" para dizer por que ele não
recebe agora. No plano, o passo 4 trabalha com quem recebe hoje: o ativo acima
do teto ocupa o espaço que já tem, e os outros dividem o resto pela nota. O
limite de concentração também não entra no ideal: ele depende do valor do
aporte e aparece só no plano.

### 5. Unidades compráveis

A unidade vem do **mercado** do ativo, não da classe:

| Mercado | Unidade |
|---|---|
| B3 (ações, FIIs, ETFs, BDRs) e bolsa americana | inteira, arredondada para baixo |
| Cripto | até 8 casas |
| Título do Tesouro Direto | centésimos de título (0,01) |
| Renda fixa por valor | centavos |

O **título do Tesouro Direto** é Renda Fixa na carteira (a meta e a fatia são
as da classe), mas tem mercado próprio: a quantidade é fracionária — o Tesouro
vende a partir de 0,01 título — e o preço é o de resgate da tabela diária
(`docs/tesouro.md`). O motor compra o que cabe em centésimos de título, para
baixo, e o que sobra do arredondamento (no máximo um centésimo de título,
algo como R$ 25 num título de R$ 2.500) entra no troco ou vai para o ativo
fracionável mais atrasado. Não modela o valor mínimo de compra do Tesouro
(R$ 30): quem monta o aporte confere na hora de comprar.

Quase sempre o mercado é o natural da classe. A exceção é o ativo marcado como
negociado na B3 (`listed_on_b3`) fora de Ações Nacionais e FIIs: um ETF de
bitcoin da B3 conta em Criptomoedas — a meta e a fatia são de cripto —, mas sai
em cota inteira e cotado em reais.

### 6. O troco

Arredondar para baixo deixa troco. Em vez de devolvê-lo, uma rodada gulosa:

1. Entre os ativos de unidade inteira com preço ≤ troco, compra **mais uma
   unidade** do que estiver mais atrás da meta — **desde que** isso aproxime da
   meta o ativo **e a classe dele** (o que falta a cada um é maior que meio
   preço). Repete.
2. Quando nenhuma unidade inteira couber, o resto vai para o ativo
   **fracionável** (cripto, título do Tesouro, renda fixa por valor) mais
   atrasado, até o que falta a ele e à classe dele.
3. O que sobrar depois disso é `change`.

Com o limite ligado, a unidade extra só sai se couber **inteira** sob o limite,
e o fracionável recebe no máximo o que falta até ele. Aqui não há a folga de
meia unidade da meta: o limite é teto duro.

A conferência da classe fecha uma brecha: um ativo pode estar abaixo da
própria fatia numa classe que, somada, já passou da meta (um FII recém-
cadastrado ao lado de um que pesa demais). Olhando só o ativo, o troco ia para
ele e a classe acima da meta aparecia "recebendo".

## Teto de preço e limite de concentração

Os dois são critério da própria pessoa, opcionais e desligados por padrão.
Nenhum vira "preço justo" de terceiros nem recomendação.

### Teto de preço por ativo (`max_price`)

Um valor em reais por unidade, gravado no ativo. Com o preço de hoje **acima**
do teto, o ativo sai da divisão daquele aporte (preço igual ao teto ainda
compra), e o plano diz por quê: `asset_above_ceiling`, com o preço e o teto.
Fora da divisão, ele continua ocupando o espaço dele na classe, como o ativo
sem nota. Se todos os ativos com nota e preço da classe estiverem acima do
teto, a classe fica bloqueada com `reason: "above_ceiling"`, e a parte dela vai
para as outras classes abaixo da meta.

Exemplo fictício: Ações Nacionais com meta de 100%, R$ 1.000 hoje e R$ 1.000
de aporte (`N` = R$ 2.000).

| Ativo | Valor hoje | Preço | Teto | Nota | Recebe |
|---|---|---|---|---|---|
| CARA3 | R$ 500 | R$ 32,50 | R$ 30,00 | 8 | nada (acima do teto) |
| BOA3 | R$ 500 | R$ 20,00 | R$ 25,00 | 4 | 50 ações, R$ 1.000 |

CARA3 ocupa R$ 500 da classe; BOA3 pode ir a R$ 2.000 − R$ 500 = R$ 1.500 e
leva o aporte inteiro. O teto é por ativo e serve para quem tem preço por
unidade (renda fixa por valor não tem). Em ativo cotado em dólar, o teto é em
reais, contra o preço já convertido.

### Sem teto: o aviso do aporte (informativo)

O teto é **opcional**, e a escolha de aportar ao preço de hoje, sem teto
nenhum, é da pessoa: nada bloqueia, nada preenche o teto sozinho e o motor não
olha para o aviso. No resultado do plano, cada ativo que **recebe** dinheiro e
**não tem teto** ganha, quando há, as referências públicas de Bazin e de
Graham (as da Análise, ver [`analise.md`](analise.md#referências-de-preço)) ao
lado do preço de hoje: a tela mostra um chip por método com o veredito ("✓
Bazin R$ 18,69" quando o preço de hoje está até o do método, "✗" quando está
acima) e a explicação no toque (ver [`design.md`](design.md)). A decisão de
aportar é da pessoa.

- É só informação, calculada **depois** do plano (`analysis::plan_references`),
  só com o que já está guardado (dividendos em cache e fundamentos gravados), sem
  rede, e só para os ativos do plano. As quantidades, os valores, as sobras e os
  avisos do motor são **idênticos** com e sem ela (há teste de integração).
- Ativo **com** teto não ganha aviso: o critério já é o da pessoa. Ativo sem
  nenhuma das duas referências (cripto, exterior, renda fixa, sem dividendo e sem
  lucro e patrimônio por ação) também não.
- O texto não diz "compre", "oportunidade", "preço justo" nem "upside": são
  referências públicas, não recomendação.

### Limite de concentração (`max_asset_percent`)

Um percentual por pessoa: nenhum ativo passa de X% de `N`, a carteira depois do
aporte — a mesma base das metas. Vale para todo ativo, inclusive renda fixa.
Quem já está acima do limite antes do aporte não recebe (o motor nunca vende).
O que o limite tira de um ativo vai primeiro para os outros da classe (passo
4); o que não cabe em nenhum deles vai para as outras classes abaixo da meta
(passo 3); o que não cabe em lugar nenhum volta como `unallocated`. O aviso
`concentration_limit` diz quem parou no limite e quanto ficou sem destino por
causa dele.

Exemplo fictício: carteira de R$ 10.000, metas 50% Ações Nacionais e 50% Renda
Fixa, R$ 2.000 de aporte (`N` = R$ 12.000; ações faltam R$ 2.000, renda fixa
está na meta).

| Ativo | Valor hoje | Nota | Sem limite | Limite 30% (R$ 3.600) | Limite 20% (R$ 2.400) |
|---|---|---|---|---|---|
| Tesouro (RF) | R$ 6.000 | 5 | — | — | — |
| AAAA3 (R$ 10) | R$ 2.000 | 8 | +R$ 2.000 (vai a 33%) | +R$ 1.600 (para em 30%) | +R$ 400 |
| BBBB3 (R$ 10) | R$ 2.000 | 2 | nada | +R$ 400 | +R$ 400 |
| Sem destino | | | R$ 0 | R$ 0 | R$ 1.200 |

- **Sem limite**, a nota 8 pede R$ 4.800 dos R$ 6.000 da classe para AAAA3, e
  o aporte inteiro vai para ele.
- **Com 30%**, AAAA3 para em R$ 3.600; os R$ 2.400 restantes da classe são de
  BBBB3, que recebe R$ 400.
- **Com 20%**, cada ação só cabe até R$ 2.400 — R$ 400 a mais cada. A renda
  fixa já está na meta, então R$ 1.200 ficam sem destino, com o aviso dizendo
  que é por causa do limite. Se outra classe ainda estivesse abaixo da meta,
  eles iriam para ela.

O limite é teto duro: toda conversão em unidade arredonda para baixo, e o troco
só compra a unidade que cabe inteira sob ele. A única folga é o centavo do
valor arredondado de cada item.

## O que o motor não usa: preço médio

**Decisão:** o motor **não** usa o preço médio, a rentabilidade nem o "caiu,
compra mais" para decidir. O preço médio é só informação para a pessoa (por
exemplo, "você paga R$ 25,00; seu PM é R$ 22,10").

Comprar mais do que caiu **porque caiu** é viés de preço médio, não critério: o
preço que a pessoa pagou no passado não diz nada sobre o ativo hoje, e baixar o
PM vira objetivo no lugar da carteira. O que decide continua sendo a meta da
classe e a nota do ativo. Quem quer reagir a preço tem o teto, que é critério
da própria pessoa sobre o preço de **hoje**, e só tira o ativo do aporte — não
puxa dinheiro para ele.

**De onde vem a quantidade.** A quantidade de cada ativo que o motor usa é a
soma do livro de lançamentos (compras, vendas, eventos, ajustes — ver
[Lançamentos](api.md#lançamentos)). O "Já comprei" grava compras no livro;
nada muda a quantidade por fora dele. Para o motor, isso não muda nada: ele lê
o valor de hoje (`quantidade × preço`), como antes. `domain/allocation.rs` não
recebe nada do livro além disso — nem o preço médio, nem o custo, nem a
rentabilidade, que saem do livro só para a pessoa ver.

## O que o resultado garante

Conferido por teste de propriedade (300 carteiras aleatórias por propriedade, a
cada execução):

- `amount = invested + change + unallocated`, ao centavo.
- Ativo com nota 0, sem preço ou acima do teto nunca recebe.
- Nenhum ativo passa da própria meta por mais de meia unidade.
- Classe acima da meta não recebe nada; a que recebe passa da meta por no
  máximo meia unidade de cada item.
- Com o limite ligado, nenhum ativo que recebe termina acima de X% de `N` (com
  a folga do centavo do arredondamento do valor).
- O aviso de quem está sem teto não entra na conta: o plano com e sem ele é o
  mesmo, número por número.
- Sem teto e sem limite, o plano é **idêntico** ao do motor anterior às duas
  regras (uma cópia congelada dele, `domain/allocation/legacy.rs`, é o
  oráculo); e um limite de 100% não muda nada.

E por dois cenários fixos (carteira fictícia de R$ 120 mil):

- **R$ 6.000**: cripto (muito abaixo da meta) leva a maior parte, os FIIs mais
  atrasados recebem cotas inteiras, e o troco fica em centavos.
- **R$ 200.000**: as classes elegíveis param na meta, e o que caberia na classe
  sem ativo aparece como `unallocated` — R$ 95.994,15, que é o buraco dela
  (R$ 96.000) menos o excesso de outra classe (R$ 5,85).

## O que o motor não olha

O motor decide só pela meta e pela nota. Renda e proventos entram **ao lado**,
nunca dentro dele:

- **Caixa no valor do aporte.** `GET /cash` devolve o que os proventos líquidos
  recebidos e as vendas do livro deixaram, menos o que aportes anteriores já
  usaram (nunca negativo), e a tela oferece "+ R$ X em caixa — incluir?".
  Aceitando, o valor digitado sobe; o motor recebe um número a mais no mesmo
  campo e roda como sempre — **o caixa só muda o valor de entrada, o motor não
  muda**. Ao registrar o aporte, `cash_used` diz quanto dele era caixa e o saldo
  baixa; o resto do aporte é dinheiro de fora e não toca o caixa
  ([`api.md`](api.md#caixa)).
- **Renda estimada no resultado.** Depois do plano pronto, cada item ganha a
  renda mensal que soma (`estimated_monthly_income`) e o plano ganha `income`
  (o que o aporte soma e a renda de antes e de depois). É informação sobre o
  plano: nenhum ativo ganha ou perde cota por render mais ou menos.

## Problemas que este desenho evita

Vistos em calculadoras de rebalanceamento por aí:

| Problema | Aqui |
|---|---|
| Limitar cada classe a `meta × N` em vez de `meta × N − atual`: com aporte grande, a classe passa da meta | teto é o que falta |
| Dinheiro sem destino que some sem aviso | `unallocated` + `class_blocked` / `concentration_limit` |
| Troco do arredondamento perdido | rodada gulosa + fracionável |
| Nota tratada só como ordem de prioridade | nota é peso da fatia |
| Limite de concentração que corta o ativo e devolve o resto | o excedente vai para os outros da classe, depois para as outras classes abaixo da meta |
| "Caiu, compra mais" pelo preço médio | o PM não entra na conta; o teto é da pessoa e só tira do aporte |
