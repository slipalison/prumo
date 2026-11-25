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
| `fixed_income` | Renda Fixa | — (nota manual) | por valor | sem cotação |
| `intl_fixed_income` | Renda Fixa Internacional | — (nota manual) | por valor | sem cotação |

Classes **por valor** não têm símbolo nem cotação: a pessoa informa quanto tem
(`amount`) e o sistema guarda como `quantity = amount`, `unit_price = 1`.

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
      "quantity": 10,
      "unit_price": 142.0,
      "manual_price": null,
      "currency": "BRL",
      "price_source": "quote",
      "quote_updated_at": "2026-11-25T12:00:00Z",
      "current_value": 1420.0,
      "class_percent": 14.36,
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

Opcionais em ambos: `display_name`, `manual_price` (só por unidade),
`score_override`. O símbolo é normalizado para maiúsculas, sem `.SA`. Responde
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
  "answers": { "4c1e…": true, "9a77…": null }
}
```

`null` em `manual_price`/`score_override` limpa o campo. Em `answers`, `true`/
`false` grava e `null` apaga a resposta. Responde `200` com o ativo.

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
    { "code": "class_blocked", "asset_class": "intl_stocks", "amount": 37800.0 }
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
- `notices[].code`: `class_blocked` (com `asset_class` e `amount` = quanto
  falta na classe), `assets_without_price` (com `symbols`),
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
  { "symbol": "KNRI11", "display_name": "Kinea Renda Imobiliária", "exchange": "SAO", "suggested_class": "br_reits" }
]
```

`suggested_class` é palpite (B3 terminando em `11` → `br_reits`, outros da B3 →
`br_stocks`, cripto → `crypto`, EUA → `intl_stocks`); a pessoa confirma.

### `POST /api/v1/quotes/refresh`

Atualiza agora as cotações dos ativos da pessoa. O servidor também atualiza
sozinho a cada 15 minutos.

```json
{ "refreshed": 8, "failed": ["XYZW11"] }
```

## Operacional (porta interna 9090)

`GET /healthz` (processo de pé), `GET /readyz` (`select 1` no Postgres),
`GET /metrics` (Prometheus). Na porta pública (8080) as três respondem 404.
