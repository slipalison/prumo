# Contrato da API

Tudo em JSON, sob `/api/v1`. Identificadores e campos em inglês (`snake_case`);
texto que a pessoa lê (`title`, `detail` dos erros) em português.

Dinheiro e quantidade trafegam como **número JSON**. Percentuais vão de 0 a 100
(não de 0 a 1). Datas com hora em RFC 3339, UTC; a data de um negócio
(`trade_date`) é `AAAA-MM-DD`, sem hora.

## Autenticação

Em produção quem autentica é o **Cloudflare Access**. A borda injeta o JWT no
cabeçalho `Cf-Access-Jwt-Assertion` (e no cookie `CF_Authorization`); o backend
confere assinatura (JWKS do team), `iss`, `aud` e `exp`, e usa o claim `email`
como identidade. Cada e-mail tem a própria carteira.

O navegador não faz nada: não há tela de login, token nem cabeçalho para
montar. Requisição sem token válido recebe `401`.

Em desenvolvimento, `AUTH_MODE=dev` aceita toda requisição como
`DEV_USER_EMAIL`. O binário só entra nesse modo quando a variável diz isso.

## Classes de ativo

Sempre as sete, sempre nesta ordem:

| `asset_class` | Rótulo | Checklist | Unidade | Cotação |
|---|---|---|---|---|
| `br_stocks` | Ações Nacionais | `stocks` | inteira | Yahoo `<SYMBOL>.SA` |
| `br_reits` | Fundos Imobiliários | `real_estate` | inteira | Yahoo `<SYMBOL>.SA` |
| `intl_stocks` | Ações Internacionais | `stocks` | inteira | Yahoo `<SYMBOL>` em USD → BRL |
| `us_reits` | REITs | `real_estate` | inteira | Yahoo `<SYMBOL>` em USD → BRL |
| `crypto` | Criptomoedas | — (nota manual) | fracionada | Yahoo `<SYMBOL>-USD` em USD → BRL |
| `fixed_income` | Renda Fixa | — (nota manual) | por valor; fração de título no Tesouro Direto | sem cotação; Tesouro: tabela do Tesouro Transparente |
| `intl_fixed_income` | Renda Fixa Internacional | — (nota manual) | por valor | sem cotação |

Classes **por valor** não têm símbolo nem cotação: a pessoa informa quanto tem
(`amount`) e o sistema guarda como `quantity = amount`, `unit_price = 1`. A
exceção é o título do Tesouro Direto, em `fixed_income` (próxima seção).

### Negociado na B3 (`listed_on_b3`)

A classe diz o que o ativo **é** para a carteira; `listed_on_b3`, onde ele é
**negociado**. Um ETF de bitcoin da B3 é cripto na carteira e ação na bolsa:
`{ "asset_class": "crypto", "symbol": "QBTC11", "listed_on_b3": true }`. Com a
marca, o ativo é cotado como papel da B3 — Yahoo `<SYMBOL>.SA`, em reais, cota
**inteira** —, seja qual for a classe. Serve para ETF de cripto, ETF de S&P 500
e BDR em Ações Internacionais, ETF de REITs, ETF de IMA-B em Renda Fixa.

- Em `br_stocks` e `br_reits` é sempre `true` (o mercado delas é a B3);
  mandar `false` nelas é `422`.
- Nas outras classes o padrão é `false`.
- Renda fixa com a marca tem ticker e cota; sem ela, nome e valor. Trocar de
  um para o outro no `PATCH` é `422`: é outro cadastro.

### Título do Tesouro Direto (`treasury`)

Renda Fixa tem dois cadastros. **Por valor** (CDB, LCI): nome e valor, sem
cotação, como sempre. **Título do Tesouro Direto**: `treasury: true`, `symbol`
é a **chave do título** — tipo e vencimento, `TD-IPCA-20350515` — e `quantity`
é a quantidade de títulos, **fracionária** (`0.35`). O preço vem sozinho da
tabela diária do Tesouro (o preço de resgate), em reais, e entra no
patrimônio como qualquer cotação. A chave é a que `GET /symbols` devolve ao
digitar "tesouro", "ipca", "selic", "prefixado", "renda+" ou "educa+"; o
`display_name` sai do tipo e do ano (`Tesouro IPCA+ 2035`).

```json
{ "asset_class": "fixed_income", "symbol": "TD-IPCA-20350515", "treasury": true, "quantity": 0.35, "score_override": 6 }
```

- `treasury: true` só vale em `fixed_income`, com a chave de um título (tipo
  conhecido, data que existe) e sem `listed_on_b3: true`: o resto é `422`.
- Sem `treasury`, Renda Fixa com símbolo é a de B3 (`listed_on_b3: true`,
  como um ETF de IMA-B); a chave de título não vira título por acidente.
- O preço é de uma data-base (`quote_updated_at` é a meia-noite dela em
  Brasília), não do pregão: o Tesouro publica um preço por dia útil. Título
  que ainda não está na tabela nasce sem preço (`price_source: "missing"`).
- Não passa pelo provedor de cotações nem pelo stream ao vivo; `POST
  /quotes/refresh` o ignora (não conta como falha).
- `manual_price` vale e vence a tabela, como nos outros ativos.
- No `PATCH`, `quantity` é a quantidade de títulos; `amount` e
  `listed_on_b3: true` são `422`. O plano compra em **centésimos de título**
  (`0.39`), o menor pedaço que o Tesouro vende.
- Um ativo por valor que já existe **não vira** título: são cadastros
  diferentes (valor informado × quantidade de títulos). Para migrar, cadastre o
  título e remova o antigo. Ver [`tesouro.md`](tesouro.md).

### Ativo que a pessoa ainda não comprou

`quantity: 0` (ou `amount: 0`) é válido e é o jeito de montar a lista de
ativos que a pessoa quer ter. Com nota e preço, o ativo entra na divisão do
aporte como qualquer outro — é assim que uma classe ainda vazia passa a
receber.

## Nota (`score`)

- Com `score_override` (0 a 10), a nota é ele (`score_source: "manual"`).
- Senão, numa classe com checklist: `max(0, sim − não)` sobre as respostas
  dadas (`score_source: "checklist"`). Pergunta sem resposta não conta.
- Senão, `0` (`score_source: "none"`).

Ativo com nota 0 não recebe aporte.

## Erros

[RFC 9457](https://www.rfc-editor.org/rfc/rfc9457), `application/problem+json`:

```json
{
  "type": "urn:prumo:error:targets_must_sum_100",
  "title": "As metas precisam somar 100%",
  "status": 422,
  "detail": "A soma enviada foi 95%.",
  "code": "targets_must_sum_100"
}
```

| `code` | status | quando |
|---|---|---|
| `unauthenticated` | 401 | sem token, token inválido ou de outra aplicação |
| `validation_failed` | 422 | corpo com campo fora do domínio (detalhe diz qual) |
| `targets_must_sum_100` | 422 | metas que não somam 100 |
| `asset_already_exists` | 409 | mesmo símbolo duas vezes na mesma classe |
| `exceeds_position` | 422 | lançamento que deixaria a posição negativa em alguma data (ver [Lançamentos](#lançamentos)) |
| `not_found` | 404 | id que não existe ou não é da pessoa |
| `unreadable_file` | 422 | planilha que quebrou o leitor no servidor (só `POST /imports/b3/preview`; o detalhe cita só o nome do arquivo) |
| `payload_too_large` | 413 | envio de planilhas acima de 10 MiB (só `POST /imports/b3/preview`) |
| `quote_provider_unavailable` | 502 | busca de símbolo sem resposta do provedor |
| `internal` | 500 | o resto (detalhe genérico; o log tem o porquê) |

## Rotas

### `GET /api/v1/info` (sem autenticação)

```json
{ "version": "0.1.0", "commit": "a1b2c3d" }
```

### `GET /api/v1/me`

```json
{ "email": "pessoa@exemplo.com" }
```

### `GET /api/v1/portfolio`

A tela inteira numa chamada.

```json
{
  "total_value": 120000.0,
  "quotes_updated_at": "2026-11-25T12:00:00Z",
  "classes": [
    {
      "asset_class": "br_reits",
      "target_percent": 5.0,
      "current_value": 9890.65,
      "current_percent": 8.24,
      "asset_count": 7,
      "eligible": true
    }
  ],
  "assets": [
    {
      "id": "8f0c…",
      "asset_class": "br_reits",
      "symbol": "KNRI11",
      "display_name": "Kinea Renda Imobiliária",
      "listed_on_b3": true,
      "treasury": false,
      "quantity": 10,
      "unit_price": 142.0,
      "manual_price": null,
      "max_price": 150.0,
      "currency": "BRL",
      "price_source": "quote",
      "quote_updated_at": "2026-11-25T12:00:00Z",
      "current_value": 1420.0,
      "class_percent": 14.36,
      "ideal_class_percent": 22.22,
      "score": 6,
      "score_source": "checklist",
      "score_override": null,
      "answers": { "4c1e…": true, "9a77…": false },
      "sector": "Tijolo",
      "subsector": null,
      "segment": "Multicategoria",
      "average_price": 130.0,
      "cost_basis": 1300.0,
      "unrealized_gain": 120.0,
      "realized_gain": 0.0,
      "return_percent": 9.23
    }
  ],
  "checklists": {
    "stocks": [{ "id": "…", "position": 1, "prompt": "…" }],
    "real_estate": [{ "id": "…", "position": 1, "prompt": "…" }]
  }
}
```

- `eligible`: a classe tem pelo menos um ativo com nota > 0, preço > 0 e
  preço até o `max_price` dele (quando há). O limite de concentração não
  entra aqui: ele depende do valor do aporte e aparece no plano.
- `max_price`: o teto de preço da pessoa, em reais por unidade (`null` = sem
  teto). Com o preço acima dele, o ativo fica fora do aporte
  ([`alocacao.md`](alocacao.md#teto-de-preço-por-ativo-max_price)).
- `price_source`: `quote` (cotação automática), `manual` (`manual_price`
  vence a cotação) ou `missing` (sem preço — vale 0 e não recebe aporte).
- `unit_price` já vem em reais. `currency` é a moeda da cotação original.
- `class_percent`: quanto o ativo representa dentro da própria classe.
- `ideal_class_percent`: a fatia que o motor mira para o ativo dentro da
  classe — `nota ÷ soma das notas` dos ativos que recebem aporte (passo 4 de
  [`alocacao.md`](alocacao.md)), de 0 a 100, com duas casas. É a mesma função
  do motor (`ideal_class_percents`), não uma segunda fórmula. Ativo sem nota
  ou sem preço vale `0`; numa classe com ao menos um ativo que recebe, a soma
  dos ideais é 100 (com a folga do arredondamento). O ideal não depende de
  quanto se aporta nem do valor dos ativos: muda com a nota e com o ativo
  ganhar ou perder preço. Também **não** muda com o `max_price`: o ideal é a
  composição-alvo pela nota, e o teto é filtro temporário de compra (o ativo
  acima dele mantém o ideal e só fica fora do aporte de hoje).
  - **O selo "abaixo do ideal" é da tela**, não do servidor: ele compara o
    valor do ativo com o ideal, e o valor muda ao vivo. A tela mede o ativo
    entre os ativos da classe que recebem (`ideal_class_percent > 0`),
    como o motor faz, e acende o selo quando falta mais de 0,5 ponto
    percentual para o ideal (a folga evita o selo piscar a cada preço novo).
    Por isso o evento `prices` leva `ideal_class_percent` junto de
    `current_value`: o selo e o "N ativos abaixo do ideal" do topo se acertam
    sozinhos, sem novo `GET /portfolio`.
- `quote_updated_at`: de quando é o preço — a hora do negócio que o formou,
  não a hora em que o Prumo buscou. Na B3 fica uns 15 minutos para trás (o
  dado gratuito da bolsa sai atrasado); fora do pregão, é a do último
  negócio. Ativo em moeda estrangeira leva a mais velha entre a do preço e a
  do câmbio. `quotes_updated_at` é a mais velha da carteira.
- `sector`, `subsector`, `segment`: onde o papel está, de fonte pública (ver
  [Saúde da carteira](#saúde-da-carteira)). Em `br_stocks` é a Classificação
  Setorial da B3 (setor, subsetor, segmento); em `br_reits`, `sector` é o
  **tipo** do fundo (`Tijolo`, `Papel`, `Fundo de fundos` ou `Híbrido`) e
  `segment` o que o administrador declara à CVM. Nulos nas outras classes e
  enquanto a fonte não respondeu ou não conhece o papel: a carteira nunca
  falha por isso.
- `quantity` é a soma do livro do ativo ([Lançamentos](#lançamentos)).
- Do livro, em reais (`null` = desconhecido, diferente de zero):
  - `average_price`: custo ÷ quantidade. `null` sem posição, com custo
    desconhecido e sempre em renda fixa por valor (lá não há "unidade").
  - `cost_basis`: custo total da posição (em renda fixa por valor, o aplicado).
  - `unrealized_gain`: `current_value − cost_basis`. `null` sem preço ou sem
    custo.
  - `realized_gain`: lucro realizado acumulado nas vendas, resgates e
    amortizações.
  - `return_percent`: `unrealized_gain ÷ cost_basis × 100`, duas casas. `null`
    com custo desconhecido ou zero.

### `GET /api/v1/portfolio/stream`

A carteira ao vivo, em [Server-Sent Events](https://html.spec.whatwg.org/multipage/server-sent-events.html)
(`text/event-stream`). Abra com `new EventSource("/api/v1/portfolio/stream")`.

```text
event: prices
data: {"total_value":120080.0,"quotes_updated_at":"2026-11-25T13:44:58Z","classes":[…],"assets":[…]}
retry: 3000
```

- Um evento `prices` sai **na hora** da conexão e outro sempre que muda um
  preço de que a carteira depende (do ativo ou do câmbio dele). Só sai evento
  quando algum número mudou.
- `classes` tem a forma de `GET /portfolio`; cada item de `assets` traz só
  `id`, `unit_price`, `currency`, `price_source`, `quote_updated_at`,
  `current_value`, `class_percent`, `ideal_class_percent`, `unrealized_gain` e
  `return_percent` — o que muda com o preço (o ideal muda quando um ativo
  ganha ou perde preço e entra ou sai da divisão; ganho e rentabilidade andam
  com o valor de hoje). O resto (preço médio, custo, lucro realizado, notas) a
  tela já tem do `GET /portfolio`.
- Comentário de keep-alive (`: keep-alive`… ) a cada 15 s, para proxy nenhum
  derrubar a conexão parada.
- O servidor **fecha** a conexão quando a credencial do Access vence e
  quando o processo vai encerrar (deploy). O `EventSource` reconecta em 3 s
  (`retry`); com a sessão vencida, a reconexão recebe o redirecionamento do
  Access e o navegador desiste — o app tenta de novo com espera crescente.
- De onde vêm os preços: o streamer do Yahoo (o WebSocket da página deles)
  para B3, EUA, cripto e dólar, gravado no banco a cada segundo; e o `chart`,
  a cada 15 minutos, para todo o resto e para moeda e nome. Vale o negócio
  mais recente, venha de onde vier. Medido em 25/11/2026: B3 com 15 minutos
  de atraso (tempo real na B3 exige licença paga de distribuidor), EUA e
  câmbio com 1–2 s, cripto a cada ~5 s. Nos EUA o streamer inclui o pré e o
  pós-mercado.

### `PUT /api/v1/goals`

```json
{ "targets": { "br_stocks": 33, "intl_stocks": 33, "br_reits": 5, "crypto": 5, "fixed_income": 24 } }
```

Classe ausente vale 0. Cada valor entre 0 e 100, até duas casas. A soma tem de
ser 100 — senão `422 targets_must_sum_100`. Responde `200` com
`{ "targets": { …as sete classes… } }`.

### `POST /api/v1/assets`

Por unidade:

```json
{ "asset_class": "br_reits", "symbol": "KNRI11", "quantity": 10 }
```

Por valor:

```json
{ "asset_class": "fixed_income", "display_name": "Tesouro IPCA+ 2035", "amount": 50000, "score_override": 7 }
```

Negociado na B3 numa classe de fora dela:

```json
{ "asset_class": "crypto", "symbol": "QBTC11", "quantity": 0, "listed_on_b3": true, "score_override": 7 }
```

Título do Tesouro Direto (ver [Título do Tesouro Direto](#título-do-tesouro-direto-treasury)):

```json
{ "asset_class": "fixed_income", "symbol": "TD-IPCA-20350515", "treasury": true, "quantity": 0.35, "score_override": 6 }
```

Opcionais em todos: `display_name`, `manual_price` e `max_price` (só por
unidade; em renda fixa por valor são `422`), `score_override`, `listed_on_b3`,
`treasury`. `max_price` é o teto de preço da pessoa, em reais por unidade,
maior que zero; vale também para título do Tesouro. `quantity`/`amount` ausente vale
0. Na B3 e nas ações americanas a quantidade é inteira; no Tesouro, em
centésimos (até duas casas).

Com o custo de entrada (opcional):

```json
{ "asset_class": "br_reits", "symbol": "KNRI11", "quantity": 10, "average_price": 130 }
{ "asset_class": "fixed_income", "display_name": "CDB ficticio", "amount": 52000, "invested": 50000 }
```

Com quantidade, o ativo nasce com um saldo inicial (`opening`) no livro,
datado hoje:
- por unidade (inclusive título do Tesouro), `average_price` (em reais, > 0)
  é o preço do saldo inicial; sem ele, o custo fica **desconhecido** até a
  pessoa informar;
- por valor, `invested` (> 0) é quanto foi aplicado: o saldo inicial é o
  aplicado (preço 1) e a diferença até `amount` entra como `adjustment` — o
  rendimento, que não é custo. Sem ele, custo desconhecido.
- `average_price` em renda fixa por valor, `invested` em ativo com cotação ou
  qualquer um dos dois com quantidade 0 é `422`.

O símbolo é normalizado para
maiúsculas, sem `.SA`. Responde
`201` com o ativo no mesmo formato de `portfolio.assets[]`. Mesmo símbolo na
mesma classe: `409 asset_already_exists`.

### `PATCH /api/v1/assets/{id}`

Qualquer subconjunto de:

```json
{
  "quantity": 26,
  "amount": 52000,
  "display_name": "Kinea Renda",
  "manual_price": null,
  "score_override": null,
  "max_price": 30,
  "answers": { "4c1e…": true, "9a77…": null },
  "listed_on_b3": true
}
```

`quantity`/`amount` diferente da atual grava um `adjustment` no livro, datado
hoje, com a diferença (o custo não muda). Compra e venda vão por `POST
/transactions`, com data e preço.

`null` em `manual_price`/`score_override`/`max_price` limpa o campo.
`max_price` maior que zero; em renda fixa por valor, `422`. Em `answers`, `true`/
`false` grava e `null` apaga a resposta. Trocar `listed_on_b3` cota o ativo no
mercado novo na hora; se a unidade passa a ser inteira, a quantidade que fica
(a enviada ou a atual) tem de ser inteira. Responde `200` com o ativo.

### `DELETE /api/v1/assets/{id}`

`204`.

### Checklists

- `POST /api/v1/checklists/{checklist}/questions` — `{ "prompt": "…" }` → `201`
  com `{ "id", "position", "prompt" }` (entra no fim).
- `PATCH /api/v1/checklist-questions/{id}` — `{ "prompt": "…" }` → `200`.
- `DELETE /api/v1/checklist-questions/{id}` → `204` (as respostas vão junto).
- `POST /api/v1/checklists/{checklist}/reset` → `200` com a lista padrão
  (apaga as perguntas da pessoa e as respostas delas).

`{checklist}` é `stocks` ou `real_estate`.

### `POST /api/v1/plans`

Calcula, não grava.

```json
{ "amount": 6000 }
```

```json
{
  "amount": 6000,
  "invested": 5999.97,
  "change": 0.03,
  "unallocated": 0,
  "total_before": 120000.0,
  "total_after": 125999.97,
  "items": [
    {
      "asset_id": "…",
      "asset_class": "crypto",
      "symbol": "BTC",
      "display_name": "Bitcoin",
      "listed_on_b3": false,
      "treasury": false,
      "quantity": 0.01214582,
      "unit_price": 345000.0,
      "value": 4190.31,
      "divisible": true,
      "class_percent_after": 100.0
    }
  ],
  "classes": [
    {
      "asset_class": "br_reits",
      "target_percent": 10,
      "percent_before": 8.24,
      "percent_after": 9.28,
      "allocated": 1809.66,
      "status": "receiving"
    }
  ],
  "notices": [
    { "code": "class_blocked", "asset_class": "intl_stocks", "amount": 37800.0, "reason": "no_assets" }
  ]
}
```

- `invested`: soma dos itens. `change`: troco do arredondamento para cotas
  inteiras. `unallocated`: dinheiro sem destino porque a classe que precisaria
  dele não tem ativo com nota (ver `notices`).
- `amount = invested + change + unallocated`, sempre.
- `status`: `receiving` (recebe neste aporte), `below_target` (abaixo da meta,
  mas o valor não chegou a comprar uma unidade), `on_target`, `above_target`,
  `blocked` (abaixo da meta sem ativo elegível) ou `no_target` (meta 0).
- `divisible`: o item aceita fração (cripto fora da B3, renda fixa por valor).
- `notices[].code`:
  - `class_blocked`, com `asset_class`, `amount` (quanto falta na classe) e
    `reason`: `no_assets` (nenhum ativo cadastrado), `unrated` (há ativos,
    nenhum com nota), `unpriced` (há ativo com nota, mas sem preço),
    `above_ceiling` (os com nota e preço estão acima do `max_price`) ou
    `concentration_limit` (os que receberiam já estão no limite por ativo);
  - `assets_without_price` e `assets_without_score`, com `symbols`;
  - `no_targets`: nenhuma meta definida, nada é comprado;
  - `asset_above_ceiling`: o ativo ficou fora deste aporte porque o preço passou
    do teto da pessoa.
    `{ "code": "asset_above_ceiling", "asset_id": "…", "symbol": "XPTO3", "unit_price": 32.5, "max_price": 30.0 }`;
  - `concentration_limit`: o limite por ativo mudou o plano. `symbols` (e
    `asset_ids`, na mesma ordem) chegaram no limite, e o que iria para eles
    foi para os outros ativos da classe ou para outras classes; `amount` é o
    que ficou sem destino por causa do limite (0 quando tudo coube).
    `{ "code": "concentration_limit", "max_percent": 15.0, "asset_ids": ["…"], "symbols": ["XPTO3"], "amount": 0 }`.

O algoritmo está em [`alocacao.md`](alocacao.md).

### `GET /api/v1/settings/allocation`

Os guarda-corpos do motor que a pessoa liga. Sem nada salvo, tudo desligado.

```json
{ "max_asset_percent": 15.0 }
```

- `max_asset_percent`: nenhum ativo passa desta fatia da carteira **depois do
  aporte** (de 0,01 a 100); `null` = sem limite. Detalhes em
  [`alocacao.md`](alocacao.md#limite-de-concentração-max_asset_percent).

### `PUT /api/v1/settings/allocation`

```json
{ "max_asset_percent": 15 }
```

O campo é obrigatório: um número de 0,01 a 100, com até duas casas, ou `null`
para desligar. Fora disso, `422 validation_failed`. Responde `200` no formato
do `GET`. O próximo `POST /plans` já usa o valor novo.

### `POST /api/v1/contributions`

Registra o que a pessoa comprou de fato — normalmente o plano, editado ou não.

```json
{ "amount": 6000, "items": [{ "asset_id": "…", "quantity": 5, "unit_price": 94.55 }] }
```

Cada item vira uma compra (`buy`, `source: "plan"`, com `contribution_id`) no
livro do ativo, datada hoje, com o preço do item — nas classes por valor, a
compra é `quantity × unit_price` em reais, a preço 1. Responde `201`:

```json
{ "id": "…", "created_at": "…", "amount": 6000, "invested": 472.75, "items": [
  { "asset_id": "…", "symbol": "BTLG11", "display_name": "BTG Logística", "quantity": 5, "unit_price": 94.55, "value": 472.75 }
] }
```

### `GET /api/v1/contributions`

Lista do mais novo para o mais antigo, no formato acima.

### `GET /api/v1/symbols?query=knri`

Busca no provedor de cotações, para o autocompletar.

```json
[
  { "symbol": "KNRI11", "display_name": "Kinea Renda Imobiliária", "exchange": "SAO", "suggested_class": "br_reits", "listed_on_b3": true, "treasury": false }
]
```

**Títulos do Tesouro Direto** vêm antes, do banco (a tabela diária), quando a
busca (3 caracteres ou mais) casa com o nome do título — cada palavra
digitada aparece no nome, em qualquer ordem: `tesouro`, `ipca`, `ipca 2035`,
`selic`, `prefixado`, `renda+`, `educa+`. Só os vigentes (não vencidos, ainda
na tabela), até 10, do tipo mais simples ao mais longo e, dentro do tipo, do
vencimento mais próximo ao mais distante:

```json
{ "symbol": "TD-IPCA-20350515", "display_name": "Tesouro IPCA+ 2035", "exchange": "Tesouro Direto", "suggested_class": "fixed_income", "listed_on_b3": false, "treasury": true }
```

Quem escolhe o título cadastra com `treasury: true` e esse `symbol`. Se o
provedor de cotações cair e já houver título para sugerir, a resposta é `200`
só com eles; sem título, continua `502 quote_provider_unavailable`.

`suggested_class` é palpite; a pessoa confirma. Na B3 o nome separa o que o
Yahoo marca igual (`EQUITY`): ETF de índice ("fundo de índice") de bitcoin/
cripto → `crypto`, de IMA-B/Tesouro → `fixed_income`, de REITs → `us_reits`,
de IFIX → `br_reits`, de S&P 500/Nasdaq/MSCI → `intl_stocks`, o resto →
`br_stocks`; BDR (código `31`–`35`/`39`, nome curto `DRN`…) → `intl_stocks`
ou `us_reits`; FII e ação como antes. Fora da B3: cripto → `crypto`, EUA →
`intl_stocks` ou `us_reits`. `listed_on_b3` é `true` em todo resultado da B3.

### `POST /api/v1/quotes/refresh`

Atualiza agora as cotações dos ativos da pessoa pelo `chart` (o título do
Tesouro fica de fora: o preço dele vem da tabela diária). O servidor
também atualiza sozinho a cada 15 minutos, e o streamer entre uma rodada e
outra (ver `GET /portfolio/stream`). Quem estiver com o stream aberto recebe
o resultado por lá.

```json
{ "refreshed": 8, "failed": ["XYZW11"] }
```

## Saúde da carteira

Avisos sobre o que a meta por classe não enxerga, com regras da própria pessoa.
**Não há nota.** O desenho, as fontes de setor e os limites estão em
[saude.md](saude.md). Tudo pelo dono, como o resto.

### `GET /api/v1/health`

```json
{
  "total_value": 9665.0,
  "warnings": [
    {
      "rule": "max_asset",
      "subject": { "kind": "asset", "asset_id": "8f0c…", "label": "ITUB4" },
      "unit": "percent",
      "current": 31.04,
      "limit": 15.0,
      "needed_contribution": 10335.0
    },
    {
      "rule": "max_sector",
      "subject": { "kind": "sector", "asset_class": "br_stocks", "sector": "Financeiro" },
      "unit": "percent",
      "current": 83.33,
      "limit": 40.0,
      "needed_contribution": 6500.0
    },
    {
      "rule": "empty_class",
      "subject": { "kind": "class", "asset_class": "crypto" },
      "unit": "count",
      "current": 0.0,
      "limit": 1.0,
      "needed_contribution": null
    }
  ],
  "rules": [
    {
      "rule": "max_asset",
      "enabled": true,
      "unit": "percent",
      "threshold": 15.0,
      "default_threshold": 15.0,
      "status": "warning"
    }
  ],
  "unclassified_count": 1,
  "sources": [
    {
      "source": "b3_stocks",
      "asset_class": "br_stocks",
      "updated_at": "2026-11-25T12:00:00Z",
      "failing": false
    }
  ]
}
```

- `warnings`: só as regras ligadas que passaram do limite, na ordem das regras
  (e, dentro de uma, do maior para o menor). O servidor manda fatos; o texto
  que a pessoa lê é montado na tela.
- `rule`: `max_asset`, `top_five`, `max_sector`, `min_assets` ou `empty_class`.
- `subject.kind`: `asset` (`asset_id`, `label`), `top_assets` (`labels`, do maior
  para o menor), `sector` (`asset_class`, `sector`) ou `class` (`asset_class`).
- `unit`: `percent` (0 a 100) ou `count` (quantidade de ativos). `current` e
  `limit` estão nessa unidade.
- `needed_contribution`: quantos reais de **dinheiro novo**, em outros ativos
  (em `max_sector`, em outros setores da mesma classe), levam o percentual de
  volta ao limite sem vender nada. `null` nas regras de contagem.
- `rules`: sempre as cinco, na ordem acima. `status`: `warning` (passou do
  limite), `ok`, `off` (desligada) ou `not_applicable` (ligada, mas a
  carteira ainda não dá o que medir: poucos ativos, sem setor, sem metas).
  `threshold` e `unit` são nulos em `empty_class`, que não tem limite.
- `unclassified_count`: ativos **com valor** em `br_stocks` e `br_reits` que a
  fonte de setor não classificou.
- `sources`: uma por fonte de setor. `updated_at` é da última busca que deu
  certo (`null` se nunca deu); `failing` diz que a última tentativa falhou e o
  setor mostrado é o da busca anterior.

### `GET /api/v1/health/rules`

```json
{
  "rules": [
    { "rule": "max_asset", "enabled": true, "unit": "percent", "threshold": 15.0, "default_threshold": 15.0 },
    { "rule": "empty_class", "enabled": true, "unit": null, "threshold": null, "default_threshold": null }
  ]
}
```

As cinco regras como a pessoa as deixou (o que ela nunca mexeu vale o padrão
de `default_threshold`).

### `PUT /api/v1/health/rules`

```json
{ "rules": { "max_asset": { "enabled": false }, "top_five": { "threshold": 70 } } }
```

Muda só as regras e os campos enviados; o resto fica como está. Cada regra
leva `enabled`, `threshold` ou os dois (vazio é `422`). Responde como o `GET`.
Tudo ou nada: uma regra inválida não grava nenhuma.

- `threshold` em `max_asset`, `top_five` e `max_sector`: percentual maior que
  0 e até 100, com até duas casas. Em `min_assets`: inteiro de 1 a 50.
- `empty_class` não tem limite: mandar `threshold` é `422`.
- Regra desconhecida, campo desconhecido ou corpo que não é objeto: `422`.

## Desempenho

Patrimônio no tempo e rentabilidade contra os índices. Método, fontes e
limites em [desempenho.md](desempenho.md). Tudo pelo dono, como o resto.

### `GET /api/v1/history/net-worth?period=12m`

O patrimônio no tempo, separado em aplicado e ganho. Uma foto por dia (horário
de Brasília), tirada depois do fechamento; antes dela, o ponto de hoje é
calculado na hora e vem com `partial: true`. Método em
[`desempenho.md`](desempenho.md).

`period`: `month` (desde o fim do mês passado), `ytd` (desde 31/12), `12m`
(padrão) ou `all`. Outro valor é `422`. Os pontos começam no fechamento de
base do período (o último em ou antes do começo dele).

```json
{
  "period": "12m",
  "starts_on": "2026-11-23",
  "points": [
    { "date": "2026-11-23", "market_value": 1000.0, "invested": 1000.0, "gain": 0.0, "partial": false },
    { "date": "2026-11-25", "market_value": 1490.0, "invested": 1500.0, "gain": -10.0, "partial": true }
  ]
}
```

- `starts_on`: a primeira foto da pessoa (`null` sem série: carteira sem
  ativo).
- `invested`: a soma dos fluxos desde `starts_on`; a primeira foto entra
  inteira. `gain = market_value - invested`.

### `GET /api/v1/performance?period=12m&benchmarks=cdi,ipca,ibov`

Rentabilidade pelo método de cotas (TWR) contra os índices, os cartões de 12
meses e desde o início, e a tabela mês × ano. Percentuais de 0 a 100, com 4
casas.

`period` como acima. `benchmarks`: lista de `cdi`, `selic`, `ipca`, `ibov`
(padrão `cdi,ipca,ibov`; índice desconhecido é `422`).

```json
{
  "period": "12m",
  "starts_on": "2026-11-23",
  "window": { "from": "2026-11-23", "to": "2026-11-25" },
  "return_percent": 1.2345,
  "benchmarks": [
    { "code": "cdi", "label": "CDI", "return_percent": 0.1016, "as_of": "2026-11-25" },
    { "code": "ipca", "label": "IPCA", "return_percent": null, "as_of": null }
  ],
  "points": [
    { "date": "2026-11-23", "portfolio": 0.0, "benchmarks": { "cdi": 0.0, "ipca": 0.0 } },
    { "date": "2026-11-25", "portfolio": 1.2345, "benchmarks": { "cdi": 0.1016, "ipca": null } }
  ],
  "summary": {
    "twelve_months": {
      "from": "2026-11-23", "to": "2026-11-25", "return_percent": 1.2345,
      "cdi_percent": 0.1016, "cdi_as_of": "2026-11-25",
      "excess_over_cdi": 1.1317, "percent_of_cdi": 1215.0, "full_window": false
    },
    "since_start": { "...": "o mesmo formato" }
  },
  "years": [
    { "year": 2026, "months": [null, null, null, null, null, null, null, null, null, null, 1.2345, null],
      "return_percent": 1.2345, "cdi_percent": 0.1016 }
  ]
}
```

- `window`: as datas que a conta usou de fato (`from` é o fechamento de base;
  se a série é mais nova que o período, é a primeira foto). `null`, com
  `return_percent: null`, sem série.
- `benchmarks[].return_percent` e os valores de `points[].benchmarks` são
  `null` quando o índice ainda não tem dado na janela (o IPCA do mês sai no
  mês seguinte); `as_of` é o último dia coberto por dado publicado.
- `points[]`: tudo acumulado desde `window.from`, um ponto por foto.
- `excess_over_cdi`: quanto a carteira rendeu acima (+) ou abaixo (−) do CDI,
  de forma composta, `(1 + r) / (1 + cdi) - 1`. `percent_of_cdi`: `r / cdi`
  em % (só com CDI positivo). `full_window`: a série cobre o período inteiro.
- `years[].months`: janeiro a dezembro; `null` onde não há série.

### `POST /api/v1/imports/b3/preview`

Lê as planilhas `.xlsx` que a Área do Investidor da B3 exporta e devolve a
**prévia** do que entraria na carteira. **Só lê: não grava nada** (a gravação
chega numa próxima versão). Formato das planilhas, mapeamento e limites em
[`importacao-b3.md`](importacao-b3.md).

`multipart/form-data` com um ou mais arquivos (qualquer nome de campo; a tela
usa `files`). Limites: **10 MiB** no corpo todo (esta é a única rota acima do
teto geral de 64 KiB), até **10 arquivos**. Os arquivos ficam só na memória do
pedido; nada do conteúdo vai para log nem para mensagem de erro.

Datas desta rota são datas de calendário `aaaa-mm-dd` (não RFC 3339); dinheiro
e quantidade são número JSON.

```json
{
  "files": [
    {
      "index": 0, "name": "negociacao.xlsx", "status": "read",
      "kinds": ["trades"], "problem": null, "message": null, "missing_columns": [],
      "rows": 5, "accepted": 4, "unrecognized": 1, "ignored": 0
    }
  ],
  "assets": [
    {
      "key": "PETR4",
      "name": "PETROLEO BRASILEIRO S.A. PETROBRAS",
      "suggested_class": "br_stocks",
      "class_basis": "name",
      "listed_on_b3": true,
      "treasury": false,
      "totals": {
        "bought_quantity": 105, "sold_quantity": 50,
        "bought_value": 3150.5, "sold_value": 1700, "net_quantity": 55,
        "income_received": 213.75, "income_provisioned": 216
      },
      "trades": [
        {
          "external_ref": "b3:v1:trade:3f2a...", "date": "2026-01-05", "side": "buy",
          "market": "fractional", "code": "PETR4", "institution": "CORRETORA EXEMPLO S/A",
          "quantity": 5, "unit_price": 30.1, "total": 150.5, "line": 3
        }
      ],
      "events": [
        {
          "external_ref": "b3:v1:event:...", "date": "2026-03-20", "kind": "bonus",
          "direction": "credit", "code": "PETR4", "product": "PETR4 - PETROLEO ...",
          "quantity": 10, "unit_price": null, "value": null, "institution": null,
          "description": "Bonificação em Ativos", "treasury": false,
          "source": "movements", "line": 6
        }
      ],
      "income": [
        {
          "external_ref": "b3:v1:income:...", "kind": "jcp", "status": "received",
          "date": "2025-12-18", "code": "PETR4", "product": "PETR4 - PETROLEO ...",
          "quantity": 150, "unit_price": 0.5, "net_amount": 63.75,
          "institution": null, "source": "movements", "line": 3
        }
      ]
    }
  ],
  "unrecognized": [
    {
      "file": 0, "sheet": "Negociação", "line": 6,
      "reason_code": "unsupported_market",
      "reason": "mercado ainda nao suportado (opcoes, termo, futuro)",
      "text": "05/03/2026 | Compra | Opção de Compra sobre Ações | ..."
    }
  ],
  "ignored": [
    { "reason_code": "settlement", "reason": "compra ou venda liquidada: vem da planilha de Negociacao", "count": 3 }
  ],
  "summary": {
    "assets": 1, "trades": 2, "events": 1, "income": 2, "unrecognized": 1, "ignored": 3,
    "first_date": "2025-12-18", "last_date": "2026-04-02"
  }
}
```

- `files[].status`: `read`, `unknown_format` (abriu, mas não é nenhuma planilha
  conhecida ou o layout mudou) ou `unreadable` (não abriu como `.xlsx`).
  `problem` diz por quê: `unknown_format`, `missing_columns` (com
  `missing_columns`), `position_sheet`, `not_xlsx`, `legacy_xls`, `corrupt`,
  `too_large`. `kinds`: `trades`, `movements`, `income_received`,
  `income_provisioned`. Um arquivo que não foi lido não derruba o envio: os
  outros seguem.
- `assets[]`, ordenado por `key`. Ativo de renda fixa do Tesouro Direto tem
  `treasury: true`, `suggested_class: "fixed_income"`, `listed_on_b3: false` e
  `key` com o nome do título (não tem código); o preço é da frente do Tesouro.
- `suggested_class` e `listed_on_b3` são sugestão pelas regras de
  `GET /symbols`; `class_basis` diz a base: `name` (nome do produto), `code`
  (só o código — a Negociação não traz nome; a tela pede para conferir) ou
  `treasury`.
- `external_ref` é estável (`b3:v1:<trade|event|income>:<hash>`): o mesmo
  negócio em dois arquivos de períodos que se sobrepõem tem a mesma chave. Linha
  repetida entre arquivos do mesmo envio entra uma vez; a outra vai para
  `ignored` como `duplicate_in_request`.
- `trades[].market`: `spot` ou `fractional` (o `F` do código já saiu).
  `events[].kind`: `split`, `reverse_split`, `bonus`, `fraction_in_kind`,
  `amortization`, `redemption`, `maturity`, `treasury_movement`.
  `income[].kind`: `dividend`, `jcp`, `yield`, `reimbursement`,
  `fraction_auction`; `net_amount` é sempre o líquido. `status`: `received` ou
  `provisioned` (`date` pode ser `null` neste).
- `unrecognized[]` são linhas que o Prumo não soube ler (`reason_code`:
  `invalid_date`, `invalid_number`, `unknown_side`, `unsupported_market`,
  `invalid_code`, `unknown_direction`, `unknown_movement`, `update`,
  `subscription`, `no_code`, `missing_value`, `missing_quantity`,
  `unknown_income_type`, `income_debit`). `ignored[]` são linhas entendidas e
  deixadas de fora de propósito (`settlement`, `custody_transfer`,
  `transferred`, `lending`, `duplicate_in_request`), contadas por motivo.
  `reason` é texto do servidor (sem acento); a tela escolhe o dela pelo
  `reason_code`.

Erros: `401` sem credencial; `413 payload_too_large` acima de 10 MiB; `422`
sem arquivo, com mais de 10 arquivos ou fora de `multipart/form-data`;
`422 unreadable_file` se a leitura de um arquivo entrar em pânico (o arquivo
é entrada não confiável; o erro cita só o nome dele, e nada de dentro).

## Lançamentos

Toda mudança de quantidade é um **lançamento** datado no livro do ativo, e a
quantidade do ativo é a soma do livro — uma fonte da verdade só. Do livro
saem o **preço médio**, o **custo**, o **lucro realizado** e a
**rentabilidade** de cada posição. A regra é a da Receita: custo médio
ponderado, com as taxas da compra somadas ao custo; a venda não muda o preço
médio. O código é `backend/src/domain/ledger.rs`.

### Tipos (`kind`)

| `kind` | Quantidade | Preço (`unit_price`) | Efeito no custo |
|---|---|---|---|
| `opening` | soma | opcional — sem ele, custo **desconhecido** | soma `qtd × preço + taxas` |
| `buy` | soma | obrigatório | soma `qtd × preço + taxas` |
| `sell` | tira (não passa da posição) | obrigatório | sai o custo médio do que saiu; lucro = `qtd × preço − taxas − custo que saiu` |
| `redemption` | tira | obrigatório | igual à venda (resgate de renda fixa, Tesouro) |
| `split` | soma as cotas novas | — | igual (desdobramento preserva o custo total) |
| `reverse_split` | tira as cotas que deixam de existir; não zera | — | igual |
| `bonus` | soma | opcional: custo atribuído por cota | soma `qtd × preço` (zero sem preço) |
| `amortization` | igual (`quantity` = cotas que receberam) | obrigatório: valor por cota | cai `qtd × preço`; o que passar do custo é lucro realizado |
| `adjustment` | soma, **com sinal** | — | igual; zera se a posição zerar |

- **Ordem do livro:** data do negócio (`trade_date`) e, no mesmo dia, a ordem
  em que o lançamento entrou. É ela que decide "a posição naquela data".
- **A venda não passa da posição** em nenhuma data — nem ao gravar, nem ao
  editar ou apagar um lançamento antigo (que muda tudo depois dele). O que
  deixaria a quantidade negativa em alguma data volta `422 exceeds_position`
  e nada é gravado. Grupamento não pode zerar a posição e amortização não pode
  contar mais cotas do que havia.
- **`adjustment`** é a correção manual: muda a quantidade e não mexe no custo.
  É o que a edição da quantidade na folha do ativo grava (`PATCH
  /assets/{id}` com `quantity` ou `amount`). Em renda fixa por valor é a
  atualização do saldo — o rendimento, que não é custo.
- **Custo desconhecido** é diferente de zero. Um `opening` sem preço deixa o
  custo desconhecido até a posição zerar: `average_price`, `cost_basis`,
  `unrealized_gain` e `return_percent` saem `null`, e a tela pede "informe o
  preço médio". Venda feita nesse meio-tempo deixa `realized_gain` `null`.
  Informar o preço do saldo inicial refaz o livro do começo e tudo aparece.
- **Renda fixa por valor** (`fixed_income`/`intl_fixed_income` sem B3): a
  quantidade é o saldo em reais e o preço é sempre 1 (mande só `quantity`; o
  servidor preenche). Os tipos são `opening`, `buy` (aplicação), `redemption`
  (resgate) e `adjustment` (atualização do saldo). Não há preço médio, só
  custo (o aplicado).
- **Unidade:** onde a cota é inteira (B3 e EUA), a quantidade de todo
  lançamento é inteira; cripto fora da B3 aceita fração. As mesmas regras de
  `POST /assets`.
- **Moeda:** `unit_price` e `fees` estão na moeda do lançamento (`currency`,
  padrão `BRL`). Em outra moeda, `fx_rate` (reais por 1 unidade da moeda, no
  dia) é obrigatório quando há preço. O custo, o preço médio e o lucro saem
  **em reais**, cada compra pelo câmbio dela: o preço médio de uma ação
  americana é o que a pessoa pagou em reais, e o ganho de hoje (`valor de hoje
  em reais − custo em reais`) inclui o câmbio. Em `BRL`, `fx_rate` é `null`.
- **Data** no futuro é `422`. O que o Prumo grava sozinho (saldo inicial do
  cadastro, ajuste da edição, "Já comprei") leva a data de hoje em Brasília.
- **`source`:** `manual` (digitado), `plan` (o "Já comprei", com
  `contribution_id`), `b3_import` (importação, com `external_ref` único por
  pessoa — reimportar não duplica) ou `migration` (criado pela migração 0004 a
  partir dos dados de antes do livro).
- **Ativo removido:** o livro dele fica como histórico, com `asset_id: null` e
  a foto de `symbol`, `asset_class` e `display_name`. Lançamento de ativo
  removido só pode ser apagado.

### `GET /api/v1/transactions?asset_id=…&from=AAAA-MM-DD&to=AAAA-MM-DD`

Os lançamentos da pessoa, do mais novo para o mais antigo (data, depois ordem
de entrada), até 5.000. Todos os filtros são opcionais; as datas incluem as
pontas. `from` depois de `to` é `422`.

```json
[
  {
    "id": "…",
    "asset_id": "…",
    "symbol": "KNRI11",
    "display_name": "Kinea Renda Imobiliária",
    "asset_class": "br_reits",
    "kind": "buy",
    "trade_date": "2026-03-02",
    "quantity": 10,
    "unit_price": 120.0,
    "fees": 10.0,
    "currency": "BRL",
    "fx_rate": null,
    "source": "manual",
    "external_ref": null,
    "contribution_id": null,
    "created_at": "2026-03-02T14:10:00Z"
  }
]
```

### `POST /api/v1/transactions`

```json
{ "asset_id": "…", "kind": "buy", "trade_date": "2026-03-02", "quantity": 10, "unit_price": 120, "fees": 10 }
```

Ativo em dólar:

```json
{ "asset_id": "…", "kind": "buy", "trade_date": "2026-03-03", "quantity": 2, "unit_price": 180, "fees": 1, "currency": "USD", "fx_rate": 5.0 }
```

`unit_price`, `fees` (padrão 0), `currency` (padrão `BRL`) e `fx_rate` seguem
a tabela acima. Responde `201` com o lançamento; a posição do ativo já vem
refeita no próximo `GET /portfolio`. Ativo que não é da pessoa: `404`.

### `PATCH /api/v1/transactions/{id}`

Qualquer subconjunto de `kind`, `trade_date`, `quantity`, `unit_price`,
`fees`, `currency` e `fx_rate`; `null` em `unit_price`/`fx_rate` limpa. O
ativo não muda. O livro inteiro é refeito; `422 exceeds_position` se a
edição deixar alguma data negativa. Responde `200` com o lançamento.

### `DELETE /api/v1/transactions/{id}`

`204`. Apagar a compra que sustenta uma venda posterior é `422
exceeds_position`.

### Custo da leitura

`assets.quantity`, `assets.cost_basis` e `assets.realized_gain` são **cache**
da posição: toda escrita no livro trava o ativo, grava, refaz a posição a
partir do livro inteiro dele e atualiza o cache **na mesma transação**. A
leitura da carteira — e o stream ao vivo, que a recalcula a cada preço que
muda — só lê o cache, numa consulta indexada por pessoa, e não refaz o livro.

Medido em 25/11/2026 (build de depuração, Postgres 17 em contêiner na mesma
máquina; carteira fictícia de 40 ativos e 20.080 lançamentos):

| | mediana |
|---|---|
| `GET /portfolio` (o que o stream faz por evento) | 6,9–7,8 ms |
| … dos quais a leitura do cache da posição | 1,4 ms |
| a mesma carteira refazendo o livro a cada leitura | 159–188 ms |
| uma escrita no livro (ativo com 500 lançamentos) | 9,4–10,3 ms |

O custo da leitura não cresce com o tamanho do livro; o da escrita cresce com
o livro **daquele ativo**, que a escrita refaz inteiro.

### De antes do livro (migração 0004)

Antes do livro, a quantidade vinha do que a pessoa digitou e do "Já comprei"
(`contribution_items`). A migração separa os dois, na mesma transação:

1. Cada item de aporte vira um `buy` (`source: "plan"`, com o
   `contribution_id`), na data do aporte em Brasília e com o preço do item.
   Em renda fixa por valor, o valor do item a preço 1. Item de ativo já
   removido não vira lançamento (não há posição) e continua no histórico de
   aportes.
2. A diferença `d = quantidade atual − Σ compras dos aportes` é o que a pessoa
   digitou:
   - `d > 0` → `opening` de `d` com **custo desconhecido** (`source:
     "migration"`), datado no cadastro do ativo (ou no primeiro aporte, se
     vier antes) e antes das compras no livro;
   - `d = 0` → nenhum saldo inicial; o custo é o das compras;
   - `d < 0` → a pessoa baixou a quantidade à mão depois de comprar: um
     `adjustment` de `d`, datado no dia da migração, depois das compras. O
     custo fica o das compras (zero se zerou); trocar o ajuste por uma venda
     com preço, na folha do ativo, acerta o lucro realizado.
3. O cache de custo sai das mesmas regras, e a migração termina conferindo
   que a soma do livro é a quantidade de **todo** ativo — se não for, ela
   falha e nada muda.

Na prática: depois da migração, todo ativo que tinha quantidade digitada
aparece com "informe o preço médio"; os que só receberam pelo "Já comprei"
já mostram preço médio e rentabilidade.

## Operacional (porta interna 9090)

`GET /healthz` (processo de pé), `GET /readyz` (`select 1` no Postgres),
`GET /metrics` (Prometheus). Na porta pública (8080) as três respondem 404.
