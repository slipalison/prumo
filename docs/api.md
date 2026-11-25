# Contrato da API

Tudo em JSON, sob `/api/v1`. Identificadores e campos em inglês (`snake_case`);
texto que a pessoa lê (`title`, `detail` dos erros) em português.

Dinheiro e quantidade trafegam como **número JSON**. Percentuais vão de 0 a 100
(não de 0 a 1). Datas em RFC 3339, UTC.

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
| `not_found` | 404 | id que não existe ou não é da pessoa |
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
      "currency": "BRL",
      "price_source": "quote",
      "quote_updated_at": "2026-11-25T12:00:00Z",
      "current_value": 1420.0,
      "class_percent": 14.36,
      "ideal_class_percent": 22.22,
      "score": 6,
      "score_source": "checklist",
      "score_override": null,
      "answers": { "4c1e…": true, "9a77…": false }
    }
  ],
  "checklists": {
    "stocks": [{ "id": "…", "position": 1, "prompt": "…" }],
    "real_estate": [{ "id": "…", "position": 1, "prompt": "…" }]
  }
}
```

- `eligible`: a classe tem pelo menos um ativo com nota > 0 e preço > 0.
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
  ganhar ou perder preço.
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
  `current_value`, `class_percent` e `ideal_class_percent` — o que muda com o
  preço (o ideal muda quando um ativo ganha ou perde preço e entra ou sai da
  divisão). O resto a tela já tem do `GET /portfolio`.
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

Opcionais em todos: `display_name`, `manual_price` (só por unidade),
`score_override`, `listed_on_b3`, `treasury`. `quantity`/`amount` ausente vale
0. Na B3 e nas ações americanas a quantidade é inteira. O símbolo é normalizado para
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
  "answers": { "4c1e…": true, "9a77…": null },
  "listed_on_b3": true
}
```

`null` em `manual_price`/`score_override` limpa o campo. Em `answers`, `true`/
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
- `notices[].code`: `class_blocked` (com `asset_class`, `amount` = quanto
  falta na classe e `reason`: `no_assets` — nenhum ativo cadastrado —,
  `unrated` — há ativos, nenhum com nota — ou `unpriced` — há ativo com nota,
  mas sem preço), `assets_without_price` (com `symbols`),
  `assets_without_score` (com `symbols`) e `no_targets` (nenhuma meta
  definida — nada é comprado).

O algoritmo está em [`alocacao.md`](alocacao.md).

### `POST /api/v1/contributions`

Registra o que a pessoa comprou de fato — normalmente o plano, editado ou não.

```json
{ "amount": 6000, "items": [{ "asset_id": "…", "quantity": 5, "unit_price": 94.55 }] }
```

Soma `quantity` em cada ativo (nas classes por valor, soma `quantity ×
unit_price` ao valor). Responde `201`:

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

## Operacional (porta interna 9090)

`GET /healthz` (processo de pé), `GET /readyz` (`select 1` no Postgres),
`GET /metrics` (Prometheus). Na porta pública (8080) as três respondem 404.
