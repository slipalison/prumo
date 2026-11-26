# Contrato da API

Tudo em JSON, sob `/api/v1`. Identificadores e campos em inglês (`snake_case`);
texto que a pessoa lê (`title`, `detail` dos erros) em português.

Dinheiro e quantidade trafegam como **número JSON**. Percentuais vão de 0 a 100
(não de 0 a 1). Datas com hora em RFC 3339, UTC; a data de um negócio
(`trade_date`) é `AAAA-MM-DD`, sem hora.

(não de 0 a 1). Instantes em RFC 3339, UTC. Datas de calendário (pagamento de
provento) em `AAAA-MM-DD`; o "hoje" que separa recebido de a receber é o de
Brasília (UTC−3).

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
| `unreadable_file` | 422 | planilha que quebrou o leitor no servidor (só `POST /imports/b3/*`; o detalhe cita só o nome do arquivo) |
| `payload_too_large` | 413 | envio de planilhas acima de 10 MiB (só `POST /imports/b3/*`) |
| `preview_stale` | 409 | `POST /imports/b3/commit` com um `digest` que não é o do plano refeito agora (a carteira ou as planilhas mudaram desde a prévia) |
| `import_blocked` | 422 | `POST /imports/b3/commit` com conflito ou classe por confirmar (o detalhe só conta quantos) |
| `quote_provider_unavailable` | 502 | busca de símbolo sem resposta do provedor |
| `tax_year_not_covered` | 422 | IR de um ano sem regra cadastrada (hoje só 2025 e 2026); nunca a regra de outro ano |
| `tax_year_before_balance` | 422 | IR de um ano antes do saldo de prejuízo informado |
| `tax_book_invalid` | 422 | o livro não fecha para o IR (o detalhe diz o ticker) |
| `darf_not_due` | 422 | marcar como pago um DARF que o Prumo não apurou a pagar |
| `too_many_requests` | 429 | pedido repetido cedo demais (só `POST /alerts/test`) |
| `telegram_unavailable` | 502 | o Telegram não aceitou o envio (só `POST /alerts/test`; ver [Alertas](#alertas)) |
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
      "return_percent": 9.23,
      "tax_instrument": "real_estate_fund",
      "tax_instrument_default": "real_estate_fund"
    }
  ],
  "profit": {
    "unrealized": 120.0,
    "realized": 0.0,
    "income_received": 85.0,
    "total": 205.0,
    "partial": false,
    "partial_assets": [],
    "removed_assets": false
  },
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

#### `profit`: lucro total

`total = realized + unrealized + income_received` — o ganho de capital
(realizado e não realizado) mais os proventos recebidos. Tudo em reais. A conta
é `backend/src/domain/profit.rs`.

- `unrealized`: a soma do `unrealized_gain` dos ativos com posição, custo e
  preço conhecidos.
- `realized`: a soma do `realized_gain` dos ativos (inclusive os já zerados).
- `income_received`: o líquido dos proventos com `pay_date` até hoje (dia de
  Brasília), de qualquer ativo — também dos que já saíram da carteira. O que
  ainda vai cair não entra.
- **O que não se sabe fica de fora, e é dito.** `partial` é `true` quando algum
  ativo não entrou inteiro, e `partial_assets` lista quem: `{ asset_id, label,
  reason }` com `reason` `unknown_cost` (posição de custo desconhecido: falta o
  preço médio), `unknown_realized` (uma venda saiu de custo desconhecido) ou
  `missing_price` (sem cotação). O ativo conta com o que se sabe dele e nunca
  vira zero ou estimativa. A tela escreve "Lucro total (parcial)".
- `removed_assets`: há venda de um ativo que já saiu da carteira. O lançamento
  fica no livro, mas sem o ativo não há como refazer a posição dele, então o
  ganho realizado dessa venda **não entra** no total. Os proventos dele entram.
- O bloco também vai no evento `prices` do stream (`profit`): o ganho não
  realizado muda com o preço.

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
  com o valor de hoje) —, e o evento leva também o bloco `profit` inteiro (o
  lucro total anda com o ganho não realizado). O resto (preço médio, custo, lucro realizado, notas) a
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
  "listed_on_b3": true,
  "tax_instrument": "etf"
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

`tax_instrument` escolhe o instrumento fiscal (ver [Impostos](#impostos));
`null` volta ao padrão. Sem escolha da pessoa, o instrumento segue o padrão
quando `listed_on_b3` ou o nome mudam; com escolha, fica.

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
  ],
  "income": { "available": true, "missing": [], "added_monthly": 35.4, "before_monthly": 410.5, "after_monthly": 445.9 }
}
```

- `items[].estimated_monthly_income`: a renda mensal que o item soma,
  **estimada** (o que a cota pagou em 12 meses × a quantidade comprada ÷ 12,
  em reais, bruto). `null` quando o ativo não tem histórico em cache (renda
  fixa, cripto, histórico ainda não buscado ou provedor fora do ar).
- `income`: o que o aporte soma por mês (`added_monthly`, a soma dos itens) e a
  renda mensal estimada da carteira antes e depois. É estimativa, nunca
  extrato; o motor não olha para ela.
  - `null` quando nenhum ativo da carteira paga provento (só renda fixa e
    cripto): não há o que estimar.
  - `available: false` quando há ativo que paga provento e nenhum tem histórico
    em cache ainda: os três valores vêm zerados e a tela diz "estimativa
    indisponível".
  - `missing`: os ativos que pagam provento e ficaram fora da conta.
- **O cálculo do plano nunca espera a rede.** A estimativa lê só o cache do
  banco (ver `GET /income/summary`); quem o enche é uma tarefa em segundo plano.

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
{ "amount": 6000, "cash_used": 812.5, "items": [{ "asset_id": "…", "quantity": 5, "unit_price": 94.55 }] }
```

`cash_used` (opcional, padrão `0`) é a parte do aporte que veio do **caixa**
([`GET /cash`](#caixa)): o saldo baixa por esse valor. O resto do `amount` é
dinheiro de fora e não toca o caixa. `cash_used` não pode passar do `amount`
nem do saldo na hora do registro (`422`, com o saldo na mensagem); a conferência
é na mesma transação que grava o aporte, sob um lock por pessoa, então dois
aportes ao mesmo tempo não gastam o mesmo dinheiro. O valor é arredondado a
centavos.

Cada item vira uma compra (`buy`, `source: "plan"`, com `contribution_id`) no
livro do ativo, datada hoje, com o preço do item — nas classes por valor, a
compra é `quantity × unit_price` em reais, a preço 1. Responde `201`:

```json
{ "id": "…", "created_at": "…", "amount": 6000, "cash_used": 812.5, "invested": 472.75, "items": [
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

> A Saúde virou a [Análise da carteira](#análise-da-carteira), com mais regras e
> dimensões. `GET /health` e `GET`/`PUT /health/rules` **continuam respondendo**
> como antes (o motor de avisos e os alertas do Telegram dependem deles) e
> compartilham a configuração com `/analysis/rules`: `/health/rules` mostra e
> grava só as sete regras originais, e a tela não os chama mais.

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
    },
    {
      "rule": "income_every_month",
      "subject": {
        "kind": "income_months",
        "empty_months": ["2026-02", "2026-04"],
        "payers": [
          { "asset_id": "3b1d…", "label": "HGLG11", "asset_class": "br_reits", "months": [2, 4] }
        ]
      },
      "unit": "count",
      "current": 10.0,
      "limit": 12.0,
      "needed_contribution": null
    },
    {
      "rule": "max_payer",
      "subject": { "kind": "payer", "asset_id": "8f0c…", "label": "KNRI11", "received": 4800.5, "total": 6000.0 },
      "unit": "percent",
      "current": 80.01,
      "limit": 25.0,
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
- `rule`: `max_asset`, `top_five`, `max_sector`, `min_assets`, `empty_class`,
  `income_every_month` ou `max_payer`.
- `subject.kind`: `asset` (`asset_id`, `label`), `top_assets` (`labels`, do maior
  para o menor), `sector` (`asset_class`, `sector`), `class` (`asset_class`),
  `income_months` ou `payer`.
  - `income_months` (`income_every_month`): `empty_months` são os meses
    `AAAA-MM` sem provento recebido entre os 12 meses fechados antes do de
    hoje, do mais velho ao mais novo; `payers` são os ativos que **ainda estão
    na carteira** e já pagaram, pelo histórico, em algum desses meses do
    calendário (`months`: 1 a 12), do que cobre mais meses para o que cobre
    menos. Nunca traz ativo de fora da carteira; pode vir vazio. `current` é
    quantos dos 12 meses tiveram provento.
  - `payer` (`max_payer`): `asset_id` (`null` se o ativo saiu da carteira),
    `label`, `received` (R$ líquido que ele pagou nos 12 meses que terminam no
    mês de hoje) e `total` (R$ líquido de todos). `current` é `received / total`
    em percentual.
- `unit`: `percent` (0 a 100) ou `count` (quantidade de ativos, ou de meses em
  `income_every_month`). `current` e `limit` estão nessa unidade.
- `needed_contribution`: quantos reais de **dinheiro novo**, em outros ativos
  (em `max_sector`, em outros setores da mesma classe), levam o percentual de
  volta ao limite sem vender nada. `null` nas regras de contagem e nas de renda.
- `rules`: sempre as sete, na ordem acima. `status`: `warning` (passou do
  limite), `ok`, `off` (desligada) ou `not_applicable` (ligada, mas a
  carteira ainda não dá o que medir: poucos ativos, sem setor, sem metas; nas
  regras de renda, menos de 3 meses de proventos recebidos).
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

As sete regras como a pessoa as deixou (o que ela nunca mexeu vale o padrão
de `default_threshold`; quem já tinha regras salvas ganha as duas de renda
ligadas, com 12 meses e 25%, sem mudar as outras).

### `PUT /api/v1/health/rules`

```json
{ "rules": { "max_asset": { "enabled": false }, "top_five": { "threshold": 70 } } }
```

Muda só as regras e os campos enviados; o resto fica como está. Cada regra
leva `enabled`, `threshold` ou os dois (vazio é `422`). Responde como o `GET`.
Tudo ou nada: uma regra inválida não grava nenhuma.

- `threshold` em `max_asset`, `top_five` e `max_sector`: percentual maior que
  0 e até 100, com até duas casas (`max_payer` também). Em `min_assets`:
  inteiro de 1 a 50. Em `income_every_month`: inteiro de 1 a 12 (meses).
- `empty_class` não tem limite: mandar `threshold` é `422`.
- Regra desconhecida, campo desconhecido ou corpo que não é objeto: `422`.

## Análise da carteira

As regras do Prumo em quatro dimensões (diversificação, dividendos,
rentabilidade e qualidade), o perfil de risco, os destaques de 12 meses e as
referências de preço. A pontuação de uma dimensão é o percentual das regras
ligadas, **com dado**, que estão ok, com o número exato. Fórmulas, limiares e o
que a Análise não faz estão em [analise.md](analise.md). Tudo pelo dono.

### `GET /api/v1/analysis`

```json
{
  "as_of": "2026-11-25",
  "total_value": 84300.0,
  "attention_count": 11,
  "dimensions": [
    {
      "id": "diversification",
      "score": { "ok": 3, "measured": 9, "no_data": 1, "percent": 33.3 },
      "rules": [
        {
          "rule": "min_abroad",
          "dimension": "diversification",
          "enabled": true,
          "unit": "percent",
          "direction": "at_least",
          "threshold": 10.0,
          "default_threshold": 10.0,
          "status": "warning",
          "current": 0.0,
          "limit": 10.0,
          "needed_contribution": 14500.0,
          "detail": null
        },
        {
          "rule": "max_asset",
          "dimension": "diversification",
          "enabled": true,
          "unit": "percent",
          "direction": "at_most",
          "threshold": 15.0,
          "default_threshold": 15.0,
          "status": "warning",
          "current": 70.76,
          "limit": 15.0,
          "needed_contribution": null,
          "detail": {
            "kind": "findings",
            "warnings": [
              {
                "rule": "max_asset",
                "subject": { "kind": "asset", "asset_id": "8f0c…", "label": "ITUB4" },
                "unit": "percent",
                "current": 70.76,
                "limit": 15.0,
                "needed_contribution": 446000.0
              }
            ]
          }
        }
      ]
    },
    {
      "id": "quality",
      "score": { "ok": 1, "measured": 2, "no_data": 6, "percent": 50.0 },
      "rules": [
        {
          "rule": "quality_roe",
          "dimension": "quality",
          "enabled": true,
          "unit": "percent",
          "direction": "at_least",
          "threshold": 70.0,
          "default_threshold": 70.0,
          "status": "warning",
          "current": 66.67,
          "limit": 70.0,
          "needed_contribution": null,
          "detail": { "kind": "quality", "covered": 2, "total": 2, "failing": ["FRAC3"] }
        }
      ]
    }
  ],
  "attention": {
    "assessed": 4,
    "without_data": 0,
    "items": [
      {
        "asset_id": "8f0c…",
        "label": "FRAC3",
        "asset_class": "br_stocks",
        "points": [
          { "kind": "judicial_recovery" },
          { "kind": "low_roe", "percent": 2.0 },
          { "kind": "debt_above_equity", "times": 2.9 }
        ]
      }
    ]
  },
  "profile": {
    "current": { "fixed_income": 70.76, "variable": 29.24, "profile": "conservative" },
    "target": { "fixed_income": 35.0, "variable": 65.0, "profile": "moderate" },
    "conservative_max": 30.0,
    "moderate_max": 70.0
  },
  "highlights": {
    "price_12m": {
      "best": [{ "asset_id": "8f0c…", "label": "KNRI11", "asset_class": "br_reits", "change_percent": 18.4 }],
      "worst": []
    },
    "since_purchase": { "best": [], "worst": [] }
  },
  "references": {
    "bazin_yield": 6.0,
    "items": [
      {
        "asset_id": "8f0c…",
        "label": "ITSA4",
        "asset_class": "br_stocks",
        "unit_price": 11.2,
        "max_price": 10.0,
        "bazin": { "price": 12.5, "annual_average": 0.75, "years": 4 },
        "graham": 9.5,
        "ceiling": 9.5
      }
    ]
  },
  "unclassified_count": 1,
  "sources": [
    { "source": "b3_stocks", "asset_class": "br_stocks", "updated_at": "2026-11-20T09:00:00Z", "failing": false }
  ]
}
```

- `attention_count`: quantas regras estão em `warning` ("N pontos a ver").
- `dimensions`: sempre as quatro, na ordem `diversification`, `dividends`,
  `returns`, `quality`, cada uma com as suas regras e a sua pontuação.
- `score`: `ok` regras ok; `measured` as ligadas com dado (ok + `warning`);
  `no_data` as ligadas sem dado, **fora** da conta; `percent = ok ÷ measured`
  (uma casa), `null` sem nenhuma medida.
- Regra (`rules[]`): `rule`, `dimension`, `enabled`, `unit` (`percent` ou
  `count`: em que o número e o limite são medidos), `direction` (`at_most`: bom
  é ficar em ou abaixo; `at_least`: em ou acima; `null`: sem limite),
  `threshold` (o da pessoa; `null` sem limite editável) e `default_threshold`.
- `status`: `ok`, `warning`, `no_data` (ligada, sem o que medir: **não é falha**)
  ou `off` (desligada). `current` é o número de agora (`null` sem dado,
  desligada ou, em `income_growth`, quando a renda anterior foi zero); `limit` é
  o que ele precisa alcançar — o limite da pessoa ou, nas regras `beat_*`, o que
  o índice rendeu; `needed_contribution` é o aporte, em reais, que leva a fatia
  de volta ao limite sem vender nada (`min_abroad`, `min_low_risk`).
- `detail` (`null` quando não há o que dizer), pelo `kind`:
  - `findings`: os avisos das sete regras originais (`warnings`, o mesmo formato
    de `GET /health`: `subject`, `current`, `limit`, `needed_contribution`);
  - `classes` (`asset_classes`), `sectors` (`sectors`);
  - `yield` (`income_12m`, `base_value`), `paying` (`paying`, `total`, `silent`),
    `income_growth` (`current_12m`, `previous_12m`), `brick` (`reits`, `brick`);
  - `benchmark` (`benchmark` `cdi`, `ibov` ou `ipca`; `horizon` `12m` ou `5y`;
    `from`, `to`, `portfolio_percent`, `benchmark_percent`);
  - `years` (`positive`, `total`), `losses` (`assets[]` com `asset_id`, `label`,
    `asset_class`, `loss_percent`);
  - `quality` (`covered`, `total`, `failing`): as regras de Qualidade, ponderadas
    pelo valor de quem tem o dado. `covered` é quantos ativos do grupo entraram
    na conta, `total` quantos o grupo tem e `failing` os que reprovaram (em
    `quality_recovery`, as empresas em recuperação judicial).
- `attention`: os pontos de atenção por ativo (do maior valor para o menor; só
  quem tem ponto). `assessed` são os ativos com fundamentos e `without_data` as
  ações e FIIs ainda sem dados. Cada ponto tem `kind` — `income_drop` (`percent`),
  `low_revenue_growth` e `low_earnings_growth` (`percent` ao ano em 5 anos),
  `low_roe` e `low_net_margin` (`percent`), `debt_above_equity` (`times`, dívida
  líquida ÷ patrimônio líquido) e `judicial_recovery` — e são fatos, não
  recomendação. Cortes em [analise.md](analise.md).
- `profile`: o perfil de agora e o das metas por classe (`null` sem ativos, ou
  sem metas); `fixed_income` + `variable` somam 100; `conservative_max` e
  `moderate_max` são os limiares de renda variável (30 e 70) que dão o rótulo
  (`conservative`, `moderate`, `aggressive`).
- `highlights`: até 3 de cada lado, só quem subiu em `best` e quem caiu em
  `worst`; ativo sem histórico não entra (`price_12m` ajusta desdobramentos).
- `references`: `bazin_yield` é o rendimento-alvo da pessoa; `items` só tem
  ativos da B3 com Bazin (histórico de dividendos) ou Graham (lucro e patrimônio
  por ação, os dois positivos; `null` se faltar um). `ceiling` é o teto pelas
  duas: o menor de `bazin.price` e `graham`, ou o que existir. Referência pública,
  não é recomendação; o teto é o `max_price` do ativo, que a tela só muda com o
  `PATCH /assets/{id}` e uma confirmação.
- `unclassified_count` e `sources`: como em `GET /health`.

Valores em reais (`needed_contribution`, `income_12m`, `base_value`,
`current_12m`, `previous_12m`, preços das referências, `received` e `total` dos
avisos) são mascarados pela tela com o olho fechado; percentuais e contagens
não.

### `GET /api/v1/analysis/rules`

```json
{
  "rules": [
    { "rule": "max_asset", "dimension": "diversification", "enabled": true, "unit": "percent",
      "direction": "at_most", "threshold": 15.0, "default_threshold": 15.0 },
    { "rule": "beat_cdi_12m", "dimension": "returns", "enabled": true, "unit": "percent",
      "direction": "at_least", "threshold": null, "default_threshold": null }
  ],
  "bazin_yield": 6.0
}
```

As 33 regras na ordem da tela, como a pessoa as deixou (o que ela nunca mexeu
vale `default_threshold`), e o rendimento-alvo do Bazin (padrão 6).

### `PUT /api/v1/analysis/rules`

```json
{ "rules": { "min_abroad": { "threshold": 15 }, "max_loss": { "enabled": false } }, "bazin_yield": 7.5 }
```

Muda só as regras e os campos enviados; o resto fica como está. Cada regra leva
`enabled`, `threshold` ou os dois; `bazin_yield` é opcional, mas o corpo precisa
de ao menos uma regra ou o `bazin_yield` (vazio é `422`). Responde como o `GET`.
Tudo ou nada: um valor inválido não grava nada.

- `threshold` de percentual: maior que 0 e até 100, com até duas casas. De
  contagem: inteiro de 1 até o máximo da regra — `min_assets` 50,
  `min_portfolio_assets` 100, `min_classes` 7, `min_sectors` 30,
  `income_every_month` 12.
- Regras de Qualidade (`quality_*`, exceto `quality_recovery`): percentual do valor,
  maior que 0 e até 100, com até duas casas (padrão 70).
- Regra sem limite editável (`empty_class`, `income_growth`, `brick_reit`,
  `quality_recovery` e as seis `beat_*`): mandar `threshold` é `422`; elas só ligam e desligam.
- `bazin_yield`: percentual maior que 0 e até 100, com até duas casas.
- Regra desconhecida, campo desconhecido ou corpo que não é objeto: `422`.

## Desempenho

Patrimônio no tempo e rentabilidade contra os índices. Método, fontes e
limites em [desempenho.md](desempenho.md). Tudo pelo dono, como o resto.

### `GET /api/v1/history/net-worth?period=12m`

O patrimônio no tempo, separado em aplicado e ganho. Uma foto por dia corrido
(horário de Brasília) desde o primeiro lançamento do livro; a de hoje é tirada
depois do fechamento e, antes dela, o ponto de hoje é calculado na hora e vem
com `partial: true`. Método em [`desempenho.md`](desempenho.md).

`period`: `month` (desde o fim do mês passado), `ytd` (desde 31/12), `12m`
(padrão) ou `all`. Outro valor é `422`. Os pontos começam no fechamento de
base do período (o último em ou antes do começo dele).

```json
{
  "period": "12m",
  "starts_on": "2026-11-23",
  "rebuilding_since": null,
  "points": [
    { "date": "2026-11-23", "market_value": 1000.0, "invested": 1000.0, "gain": 0.0, "income": 0.0, "partial": false },
    { "date": "2026-11-25", "market_value": 1490.0, "invested": 1500.0, "gain": -10.0, "income": 12.5, "partial": true }
  ]
}
```

- `starts_on`: o primeiro dia da série, que é o do lançamento mais antigo
  (`null` sem série: livro vazio).
- `rebuilding_since`: o livro mudou no passado (importação, lançamento antigo
  editado ou apagado) e a série está sendo refeita em segundo plano desde
  essa data; os pontos ainda são os de antes. `null` quando a série está em
  dia. A tela pergunta de novo enquanto não for `null`.
- `invested`: a soma dos fluxos desde `starts_on` — o que a pessoa pôs
  (compras com taxas) menos o que tirou (vendas, resgates, amortizações).
  `gain = market_value - invested`, o ganho de preço.
- `income`: proventos líquidos recebidos (pela data de pagamento) desde
  `starts_on`. Não estão em `gain`; estão na rentabilidade.

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
  "rebuilding_since": null,
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

- `rebuilding_since`: como em `GET /history/net-worth`.
- `return_percent`: pelo método de cotas, com os proventos recebidos como
  retorno (não como aporte).
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

Lê as planilhas `.xlsx` que a Área do Investidor da B3 exporta, confronta com
a carteira da pessoa e devolve a **prévia** com o **plano**: o que entra, o
que já existe e o que conflita. **Só lê: não grava nada.** Formato das
planilhas, mapeamento e regras em [`importacao-b3.md`](importacao-b3.md).

`multipart/form-data` com um ou mais arquivos (qualquer nome de campo com
`filename`; a tela usa `files`) e, opcional, uma parte de texto `decisions`
com as escolhas da pessoa, em JSON (até 256 KiB):

```json
{
  "assets": {
    "KNRI11": { "asset_class": "br_reits" },
    "HGLG11": { "opening": { "quantity": 5, "unit_price": 100 } },
    "BOVV11": { "skip": true }
  }
}
```

- a chave é o `key` do ativo na prévia;
- `asset_class`: a classe de um ativo **novo** da B3 (num ativo que já existe
  e no Tesouro, ignorada). Com `class_basis: "code"`, a gravação exige esta
  confirmação;
- `opening`: o saldo inicial que a pessoa escolheu no lugar do sugerido
  (`quantity` 0 = sem saldo inicial; `unit_price` nulo = custo desconhecido);
  de 0 a 10^15;
- `skip`: deixa o ativo inteiro (lançamentos e proventos) de fora.

Limites: **10 MiB** no corpo todo (estas são as únicas rotas acima do teto
geral de 64 KiB), até **10 arquivos**. Os arquivos ficam só na memória do
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
  },
  "plan": {
    "digest": "9c1f0a3b5d7e9f11a2b4c6d8e0f1a3b5",
    "ready": true,
    "assets": [
      {
        "key": "PETR4", "action": "update", "asset_id": "0b6c…", "symbol": "PETR4",
        "display_name": "Petrobras", "asset_class": "br_stocks", "class_basis": "name",
        "listed_on_b3": true, "treasury": false,
        "class_editable": false, "class_needs_confirmation": false,
        "current_quantity": 120, "final_quantity": 125,
        "opening": {
          "date": "2026-01-04", "quantity": 70, "unit_price": null,
          "suggested_quantity": 70, "suggested_unit_price": null,
          "basis": "keeps_quantity", "chosen": false
        },
        "replaced": [ { "kind": "opening", "date": "2026-05-01", "quantity": 120, "unit_price": null } ],
        "conflict": null,
        "counts": {
          "new_transactions": 2, "existing_transactions": 0, "new_income": 1,
          "replaced_income": 0, "existing_income": 0, "not_imported": 1
        }
      }
    ],
    "lines": {
      "b3:v1:trade:3f2a...": { "status": "new", "reason_code": null },
      "b3:v1:income:...": { "status": "existing", "reason_code": "same_ref" }
    },
    "summary": {
      "assets_created": 0, "assets_updated": 1, "assets_skipped": 0,
      "transactions_new": 2, "transactions_existing": 0,
      "income_new": 1, "income_replaced": 0, "income_existing": 0,
      "openings": 1, "not_imported": 1, "conflicts": 0, "needs_confirmation": 0
    }
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
- `plan` é o que a gravação faria com estes arquivos, estas decisões e a
  carteira de agora:
  - `plan.lines` dá, por `external_ref`, o destino de cada linha de `assets[]`:
    `status` `new` (entra), `existing` (já está no livro ou na Renda),
    `conflict` (o livro recusaria) ou `not_imported` (fica de fora), e o
    `reason_code`: `same_ref` (já importada), `same_entry` (é o lançamento
    digitado igual: mesmo ativo, tipo, data, quantidade e preço até 2% —
    o "Já comprei"), `same_income`, `replaces_provisioned` (entra no lugar do
    provento a receber), `replaces_automatic` (entra no lugar do provento
    automático do mesmo pagamento e valor), `superseded_by_received`, `provisioned_without_date`,
    `asset_skipped`, `treasury_unidentified`, `treasury_interest`,
    `fraction_in_kind`, `event_debit`, `reverse_split_unclear`,
    `missing_quantity`, `missing_value`, `exceeds_position`, `unit`, `invalid`.
  - `plan.assets[]`, por `key`: `action` `create` (o ativo nasce), `update`
    (já existe e ganha lançamento ou provento), `skip` (a pessoa deixou de
    fora) ou `nothing`; o símbolo que será gravado (o título do Tesouro já como
    `TD-<tipo>-<vencimento>`); a classe (num ativo que já existe, a dele;
    `class_editable` só em ativo novo da B3; `class_needs_confirmation` trava a
    gravação até vir `asset_class` nas decisões); `current_quantity` e
    `final_quantity` (depois de gravar; nulo com conflito ou sem lançamento
    novo); `opening`, o saldo inicial do ativo depois da importação (datado na
    véspera do primeiro lançamento; `basis` `covers_sales`, `keeps_quantity`
    ou `none`; `chosen` quando veio das decisões); `replaced`, os saldos
    iniciais e ajustes digitados que ele substitui; e `conflict`
    (`reason_code` `exceeds_position`, `invalid_opening`, `unit`, `invalid`;
    `detail` é texto do servidor, com número — a tela usa o dela).
  - `plan.ready`: sem conflito e sem classe por confirmar (ativo deixado de
    fora não trava). `plan.digest` resume tudo o que seria gravado; a gravação
    pede o mesmo.

Erros: `401` sem credencial; `413 payload_too_large` acima de 10 MiB; `422`
sem arquivo, com mais de 10 arquivos, fora de `multipart/form-data` ou com
`decisions` inválidas; `422 unreadable_file` se a leitura de um arquivo entrar
em pânico (o arquivo é entrada não confiável; o erro cita só o nome dele, e
nada de dentro).

### `POST /api/v1/imports/b3/commit`

Grava o plano conferido, **tudo ou nada**. O servidor não guarda nada entre a
prévia e a gravação: a tela reenvia **os mesmos arquivos e as mesmas
decisões** da última prévia, mais o `digest` dela na parte `decisions`:

```json
{ "digest": "9c1f0a3b5d7e9f11a2b4c6d8e0f1a3b5", "assets": { "KNRI11": { "asset_class": "br_reits" } } }
```

O servidor relê os arquivos, trava a carteira da pessoa, refaz o plano ali
dentro e grava só se o `digest` for o mesmo. Numa transação: cria os ativos
novos, troca os saldos iniciais e ajustes digitados pelo saldo inicial do
plano, grava os lançamentos (`source: "b3_import"`, `external_ref` da linha)
e refaz a posição de cada ativo, dá o `external_ref` ao lançamento digitado
que bateu com a linha, grava os proventos (`source: "b3_import"`) e troca o
provisionado pelo recebido. Qualquer erro desfaz tudo. Reimportar os mesmos
arquivos grava zero.

```json
{
  "summary": { "assets_created": 3, "assets_updated": 1, "transactions_new": 6, "income_new": 3, "income_replaced": 1, "...": "como plan.summary" },
  "not_imported": [ { "key": "WEGE3", "external_ref": "b3:v1:income:...", "reason_code": "provisioned_without_date" } ],
  "skipped_assets": ["BOVV11"],
  "unrecognized": 2,
  "ignored": 4
}
```

Depois de gravar: a série de desempenho fica marcada para refazer desde o
lançamento ou provento mais antigo que a gravação escreveu ou apagou (os
gatilhos da série, ver [Desempenho](#desempenho)), e a tarefa de
reconstrução a refaz inteira dali em diante; os ativos novos buscam cotação e
histórico de dividendos em segundo plano. O caixa (`GET /cash`) só conta o que
é de `cash_since` em diante: importar anos de histórico não o enche de
proventos e vendas antigos. A resposta sai com `Cache-Control: no-store`.

Erros: os de `preview`; `422 validation_failed` sem `digest`;
`409 preview_stale` quando o plano refeito não é o conferido (nada gravado:
confira a prévia de novo); `422 import_blocked` com conflito ou classe por
confirmar (nada gravado); `409 asset_already_exists` se outro pedido criou o
mesmo ativo no meio (nada gravado).

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
  lançamento é inteira; no título do Tesouro, em centésimos (0,01 vale, 0,001
  é `422`); cripto fora da B3 aceita fração. As mesmas regras valem em `POST
  /assets`, no `PATCH` da quantidade e no "Já comprei". O título é por
  unidade: tem preço médio, compra e venda como uma ação.
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
  pessoa — reimportar não duplica) ou `migration` (criado pela migração 0010 a
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

### De antes do livro (migração 0010)

A migração é a `0010`, depois da `0008` (título do Tesouro), de propósito: a
conta separa renda fixa por valor de título do Tesouro pela coluna
`assets.treasury`, que só existe a partir da 0008. O sqlx aplica uma versão
pendente menor que a última aplicada (há teste disso: é o caso da 0006, 0007 e
0009, que entram em produção depois da 0008), mas com número 0004 a migração
do livro rodaria antes da 0008 num banco novo e depois dela em produção, com
esquemas diferentes; com 0010 ela vê o mesmo esquema nos dois. O teste da
migração monta o banco como está no ar (0001–0003 e 0008), põe dados
fictícios e roda o resto como o próximo deploy roda.

Antes do livro, a quantidade vinha do que a pessoa digitou e do "Já comprei"
(`contribution_items`). A migração separa os dois, na mesma transação:

1. Cada item de aporte vira um `buy` (`source: "plan"`, com o
   `contribution_id`), na data do aporte em Brasília e com o preço do item.
   Em renda fixa por valor, o valor do item a preço 1; título do Tesouro é
   por unidade (centésimos), como ação. Item de ativo já
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

## Renda

Proventos são o dinheiro que a carteira paga: dividendo, JCP, rendimento de FII
e amortização. Cada um é um **evento** com data de pagamento; recebido ou a
receber **não é gravado**, sai da `pay_date` contra o dia de hoje (pagamento
hoje já é recebido). Tudo é filtrado pela pessoa dona; id de outra pessoa é
`404`.

### O evento (`IncomeEvent`)

```json
{
  "id": "…",
  "asset_id": "8f0c…",
  "symbol": "ITSA4",
  "display_name": "Itaúsa",
  "kind": "jcp",
  "ex_date": "2026-11-10",
  "pay_date": "2026-11-30",
  "amount_per_unit": 0.5,
  "quantity": 200,
  "gross": 100.0,
  "withholding": 15.0,
  "net": 85.0,
  "status": "upcoming",
  "source": "manual",
  "external_ref": null
}
```

- `kind`: `dividend`, `jcp`, `reit_income`, `amortization` ou `other`.
- `gross = amount_per_unit × quantity` (centavos). `withholding` é o IR retido
  na fonte: **no JCP, 15% do bruto até 2025 e 17,5% desde 2026** (pela
  `pay_date`; LC 224/2025), zero nos demais (dividendo e rendimento de
  FII são isentos para pessoa física); `net = gross − withholding`.
- `status`: `received` ou `upcoming`, pela `pay_date`.
- `source`: `manual` (lançado pela pessoa), `b3_import` (extrato da B3),
  `b3_announcement` (o **automático**: anúncio da B3 × livro na data com, ver
  [Proventos automáticos](#proventos-automáticos)) ou `estimate`. Só `manual`
  se edita; `external_ref` é único por pessoa (a importação repetida não
  duplica; no automático é `b3a:<isin>:<rótulo>:<data com>:<valor>`).
- `asset_id` vira `null` se o ativo for apagado da carteira; o evento guarda
  `symbol` e `display_name`, e a renda passada continua de pé.

### `GET /api/v1/income`

Lista do pagamento mais novo para o mais velho. Filtros, todos opcionais:
`from` e `to` (`AAAA-MM-DD`, na `pay_date`, inclusivos), `status`
(`received` ou `upcoming`), `asset` (id do ativo) e `limit` (1 a 1000, padrão
200). Filtro inválido é `422`.

### `POST /api/v1/income`

Lança um provento à mão.

```json
{ "asset_id": "…", "kind": "reit_income", "pay_date": "2026-11-14", "amount_per_unit": 1.1, "quantity": 10 }
```

- Obrigatórios: `asset_id` (da pessoa), `kind`, `pay_date`, `amount_per_unit`.
- Opcionais: `quantity`, `ex_date` (não pode passar da `pay_date`) e
  `withholding` (substitui a regra do tipo; entre 0 e o bruto).
- **Sem `quantity`, vale a posição do livro na data com** — não a de hoje. A
  data com é o dia anterior à `ex_date` (o último em que comprar dava direito); sem
  `ex_date`, o dia da `pay_date` (no máximo hoje). A posição é o replay do livro
  até essa data (`domain/entitlement.rs`):
  - lançamento datado na `ex_date` ou depois não conta (comprou no dia da data
    ex: sem direito; comprou na data com: com direito);
  - **desdobramento e grupamento anteriores à data com já estão na quantidade**
    (100 cotas que viraram 200 em março valem 200 num provento de junho);
  - o **saldo inicial** (`opening`) vale desde sempre: a data dele é a do
    cadastro, não a da compra, e as cotas já existiam antes do livro. Compras,
    vendas e ajustes valem na data deles;
  - quem não tinha cota na data com recebe `422` ("Voce nao tinha cotas desse
    ativo na data com…") e pode lançar mesmo assim informando `quantity`;
  - renda fixa por valor não tem cota: o valor recebido é "1 × valor", como
    antes.
  A `quantity` do corpo sempre vale mais que o livro.

### `GET /api/v1/income/entitlement?asset=…&ex_date=AAAA-MM-DD&pay_date=AAAA-MM-DD`

As cotas que o servidor usaria, para a tela mostrar antes de lançar. `asset`
é obrigatório (de outra pessoa é `404`); `ex_date` e `pay_date` são opcionais
(sem as duas, vale hoje).

```json
{ "quantity": 15.0, "record_date": "2026-03-09" }
```

`record_date` é a data com usada; `quantity`, a posição do livro nela.
- `pay_date` entre 2000 e 2100; valor por cota e quantidade maiores que zero; o
  bruto precisa chegar a um centavo. Responde `201` com o evento.

### `PATCH /api/v1/income/{id}`

Qualquer subconjunto de `kind`, `ex_date` (`null` limpa), `pay_date`,
`amount_per_unit`, `quantity` e `withholding`. Os valores são recalculados; sem
`withholding` no corpo, o IR volta à regra do tipo. Sem `quantity` no corpo, a
quantidade **continua a do evento** — o `PATCH` não a recalcula pelo livro, nem
quando muda a data. Só evento `manual`:
os de extrato respondem `422`. `200` com o evento.

### `DELETE /api/v1/income/{id}`

`204`. Apaga qualquer evento da pessoa (um evento importado volta se o extrato
for importado de novo). O automático apagado **não volta**: o `external_ref`
dele fica lembrado e a rodada não o recria.

### `GET /api/v1/income/summary`

O que a tela Renda precisa, numa chamada.

```json
{
  "today": "2026-11-25",
  "total_12m": 1620.0,
  "monthly_average": 135.0,
  "upcoming_total": 90.0,
  "months": [{ "month": "2025-12", "received": 120.0, "upcoming": 0.0 }],
  "by_payer": [{
    "asset_id": "…", "label": "KNRI11", "total": 972.0, "percent": 60.0,
    "yield_on_cost": { "income_12m": 972.0, "basis": "received", "cost": 9000.0, "percent": 10.8, "gap": null, "message": null }
  }],
  "years": [{ "year": 2026, "months": [0, 125, 130, 0, 140, 145, 0, 155, 160, 0, 170, 240], "total": 1265.0 }],
  "upcoming": [],
  "estimate": {
    "monthly": 410.5,
    "updated_at": "2026-11-25T11:00:00Z",
    "assets": [{ "asset_id": "…", "symbol": "KNRI11", "display_name": "Kinea", "quantity": 10, "per_unit_12m": 12.6, "monthly": 10.5, "trailing_12m": 126.0 }],
    "missing": ["XPTO3"]
  },
  "yield_on_cost": {
    "income_12m": 972.0, "basis": "received", "cost": 9000.0, "percent": 10.8, "gap": null, "message": null,
    "partial": true, "unknown_cost": ["WEGE3"]
  },
  "goal": null
}
```

- `months`: os 12 meses civis que terminam no mês de hoje, do mais velho ao
  atual. `received` é o líquido com `pay_date` até hoje; `upcoming`, o líquido
  do mesmo mês que ainda vai cair (só o mês corrente pode ter).
- `total_12m`: soma do **recebido** nos 12 meses. `monthly_average = total_12m
  ÷ 12`. `upcoming_total`: tudo que ainda vai cair, de qualquer mês.
- `by_payer`: o recebido nos 12 meses por ativo, do maior para o menor, com a
  parte (`percent`, soma 100) de cada um e o seu `yield_on_cost` (abaixo).
- `years`: todos os anos com evento, do mais novo ao mais velho; `months` vai de
  janeiro a dezembro e soma recebido e a receber.
- `upcoming`: até 20 eventos com pagamento depois de hoje, do mais perto ao
  mais longe.
- `goal`: a meta de renda com o progresso (ver abaixo), ou `null`.

#### `estimate`: a renda estimada da carteira

**É estimativa, não extrato.** Vem do histórico de dividendo por cota do Yahoo
(`chart` com `events=div`): data ex e valor bruto, sem data de pagamento e com
JCP misturado ao dividendo nas ações da B3. Para cada ativo cotado na B3 ou nos
EUA, `per_unit_12m` é o que a cota pagou nos últimos 365 dias (em reais; valores
em dólar usam o câmbio da carteira) e `monthly = per_unit_12m × quantity ÷ 12`,
com a quantidade **de hoje**: é o ritmo da carteira atual, e o plano soma o
que o aporte compra por cima dele (a renda projetada do plano usa a quantidade
depois do aporte, não a da data com).

`trailing_12m` é a outra conta, a que olha para trás: o que a posição teria
rendido nos últimos 365 dias (bruto, em reais) com as cotas que o **livro tinha
na data com de cada dividendo** — cada dividendo do histórico vezes a posição
daquela data, não 12 meses de dividendo vezes a quantidade de hoje. O histórico do
Yahoo é ajustado por desdobramento (o dividendo antigo de uma ação que dobrou
aparece pela metade), então a quantidade da data com é levada à **base de hoje**
multiplicando pelos desdobramentos e grupamentos posteriores à data ex
(bonificação não entra: o provedor a trata de forma diferente de uma emissora
para outra). Dividendo de um dia em que a pessoa não tinha cota não conta.
`trailing_12m` é `null` quando o livro não fecha. É o que alimenta o yield on
cost quando ainda não há nada recebido.
Renda fixa por valor e cripto não entram. `missing` lista os papéis sem
histórico (provedor fora do ar ou sem câmbio), que ficam fora da conta.

**Nenhuma rota espera pelo provedor.** O resumo e o plano leem só o histórico
guardado no banco (`dividend_history`, compartilhado entre as carteiras); sem
ele, o ativo entra em `missing` e a estimativa fica de fora (`monthly` zerado se
nenhum tiver). Quem enche o cache é o servidor, em segundo plano:

- ao criar um ativo (ou trocar o mercado dele), a busca desse símbolo é
  disparada na hora, sem a resposta esperar;
- uma tarefa periódica (a primeira ao subir o processo, depois a cada hora)
  busca todo símbolo de todas as carteiras que nunca foi buscado ou tem mais de
  24 horas, no mesmo esquema do refresher de cotações. Símbolo que falhou
  descansa 10 minutos.

Gravar é idempotente (lock por símbolo e upsert), então duas réplicas buscando o
mesmo símbolo ao mesmo tempo terminam no mesmo estado. Com o provedor fora do ar
vale o que já está guardado. O que a pessoa lançou ou importou nunca é
substituído por esta estimativa.

### Yield on cost

`yield on cost = renda de 12 meses ÷ custo × 100`, onde o custo é o
`cost_basis` do livro (o que a pessoa pagou, com taxas — não o preço de hoje). A
conta é `backend/src/domain/yoc.rs`. Aparece em três lugares: em cada pagador de
`by_payer`, no `yield_on_cost` do resumo (a carteira) e em
`GET /api/v1/income/yield-on-cost` (cada ativo, para a folha dele).

```json
{ "income_12m": 972.0, "basis": "received", "cost": 9000.0, "percent": 10.8, "gap": null, "message": null }
```

- `basis: "received"`: a renda é o **líquido recebido** nos 12 meses
  (`by_payer`). Só quando nada foi recebido nesse período a renda vem de
  `trailing_12m` (bruta, estimada) e `basis` é `"estimate"` — o dado do livro
  sempre vence a estimativa, a mesma regra da meta de renda.
- **Custo desconhecido** (saldo inicial sem preço médio) não vira zero nem
  chute: `percent` é `null`, `gap` é `unknown_cost` e `message` é
  `"informe o preco medio"`; `cost` também é `null`. A renda continua
  aparecendo. Os outros motivos: `zero_cost` (há posição, mas o custo é zero —
  só bonificação) e `no_position` (ativo zerado ou removido, que ainda tem renda
  nos 12 meses).
- **Carteira** (`yield_on_cost` no resumo e `portfolio` no relatório): a renda dos
  ativos de custo conhecido sobre o custo deles, contando também o custo dos que
  não renderam (puxam o yield para baixo, como deve). Ativo sem posição não
  entra; ativo de custo desconhecido sai da renda **e** do custo, `partial` vira
  `true` e `unknown_cost` lista quem ficou de fora. `cost` é a soma dos custos
  que entraram. Sem nenhum ativo de custo conhecido, `percent` é `null`.
- Limite conhecido: a renda dos 12 meses é dividida pelo custo **de hoje**. Quem
  comprou mais cotas há poucos meses tem o custo cheio e a renda de uma posição
  menor; o número sai baixo até completar um ano.

### `GET /api/v1/income/yield-on-cost`

```json
{
  "portfolio": { "income_12m": 972.0, "basis": "received", "cost": 9000.0, "percent": 10.8,
                 "gap": null, "message": null, "partial": false, "unknown_cost": [] },
  "assets": [{ "asset_id": "…", "income_12m": 972.0, "basis": "received", "cost": 9000.0,
               "percent": 10.8, "gap": null, "message": null }]
}
```

Um item por ativo da carteira, na ordem dela (inclusive os que não renderam:
`income_12m` é `0`, e a tela só mostra a linha de quem rendeu).

### Meta de renda: `/api/v1/income/goal`

A meta de renda mensal (`kind = monthly_income` na tabela `objectives`). Uma por
pessoa.

- `GET`: a meta com o progresso, ou `null` sem meta.
- `PUT { "target": 800 }`: cria ou troca o valor (maior que zero) e zera a data
  de conquista. `200` com a meta.
- `DELETE`: `204`; `404` sem meta.

```json
{
  "id": "…",
  "target": 800.0,
  "achieved_at": null,
  "current_monthly": 135.0,
  "basis": "received",
  "percent": 16.88,
  "reached": false
}
```

`current_monthly` é a média mensal do **recebido** (`basis: "received"`) ou,
enquanto não há nada recebido nos 12 meses, a estimativa da carteira
(`basis: "estimate"`). `percent` pode passar de 100. `reached` só vale com o
recebido: a estimativa não conquista meta. Na primeira vez que o recebido
alcança a meta, `achieved_at` guarda o instante e fica.

### Proventos automáticos

O Prumo calcula os proventos de cada ativo da B3 da pessoa: o anúncio da
empresa (página pública de eventos da B3, buscada uma vez por dia em segundo
plano) vezes a quantidade do livro na data com. Cada um vira um `IncomeEvent`
com `source: "b3_announcement"`, recebido ou a receber pela `pay_date`, e conta
na Renda, no caixa, no desempenho, no IR e nos alertas como qualquer outro.
Regra inteira (fonte, data com, tipos, deduplicação, recálculo, limites) em
[proventos-automaticos.md](proventos-automaticos.md). Em resumo:

- quantidade = posição do livro no fim da data com (`lastDatePrior`), a mesma
  regra de `GET /income/entitlement`; zero, nada;
- `DIVIDENDO` → `dividend`; `JRS CAP PROPRIO` → `jcp`, com o IR retido pela
  `pay_date` (15% até 2025, 17,5% desde 2026); `RENDIMENTO` → `reit_income` em
  FII e `other` em ação ou ETF; amortização → `amortization`; o resto → `other`;
- anúncio sem data de pagamento fica guardado e só vira provento quando a
  empresa define a data;
- igual a um `manual` ou `b3_import` do mesmo ativo (tipo, data com ou de
  pagamento, valor por cota a 1%) não é criado: o da pessoa vence. O
  importado que chega depois toma o lugar do automático
  (`reason_code: replaces_automatic` na prévia);
- de hora em hora a rodada refaz os automáticos: livro mudado antes da data com
  refaz a quantidade; posição zerada ou anúncio cancelado, o automático some.

#### `GET /api/v1/income/auto`

```json
{
  "enabled": true,
  "fetched_at": "2026-11-26T09:00:00Z",
  "covered": ["ITSA4", "KNRI11"],
  "unmatched": ["XPTO11"],
  "pending_events": [
    { "asset_id": "…", "symbol": "ITSA4", "kind": "bonus", "record_date": "2025-12-18", "factor": 2.0, "quantity": 140.0 }
  ]
}
```

- `enabled`: a chave da pessoa (ligada por padrão).
- `fetched_at`: a busca mais velha entre os emissores da carteira; `null`
  antes da primeira.
- `covered`: ativos da B3 da pessoa com ISIN conhecido; `unmatched`: os sem
  (sem anúncio na B3, ou código que a regra não resolve) — ficam sem
  automático.
- `pending_events`: bonificação (`bonus`), desdobramento (`split`) e
  grupamento (`reverse_split`) que a B3 anunciou nos últimos 18 meses, com cota
  na data com, depois do saldo inicial e ainda sem o lançamento do mesmo tipo
  no livro (até 90 dias depois da data com). **Não são lançados sozinhos**
  (falta o custo atribuído): a tela pede para lançar. `factor` é como a B3
  publica: percentual na bonificação e no desdobramento (`100` = cada ação vira
  duas), multiplicador no grupamento (`0.1` = dez viram uma). `quantity` é a
  posição do livro no fim da data com.

#### `PUT /api/v1/income/auto`

`{ "enabled": false }` apaga os automáticos da pessoa na hora (os `manual` e
`b3_import` ficam); `{ "enabled": true }` os refaz na hora com os anúncios que
já estão no banco, sem ir à B3. `200` com o mesmo corpo do `GET`; campo que
não é booleano, `422`.

## Caixa

O dinheiro que a carteira gerou e a pessoa ainda não reaplicou. O campo do aporte
oferece o saldo ("+ R$ X em caixa — incluir?"); o motor do aporte **não muda**,
só o valor de entrada (ver [`alocacao.md`](alocacao.md#o-que-o-motor-não-olha)).
A conta é `backend/src/domain/cash.rs`.

**Entra:** o provento líquido recebido (`pay_date` até hoje) e o valor líquido das
vendas e resgates do livro (`quantidade × preço − taxas`, em reais; venda com taxa
maior que o valor rende zero). **Sai:** o `cash_used` de cada aporte registrado.
A amortização do livro não entra (ela devolve capital que a pessoa lança também
como provento `amortization`, e contar as duas dobraria o dinheiro); para uma
amortização virar caixa, lance-a como provento.

**Calculado, não guardado.** Não há tabela `cash_balance`: o saldo é refeito a
cada leitura, dos eventos de renda, do livro e dos aportes (`contributions.cash_used`).
Uma tabela de saldo seria uma segunda fonte da verdade, errada na primeira edição
ou exclusão de um provento ou de uma venda; o livro inteiro é refeito justamente
para isso não acontecer.

**Nunca negativo.** O replay vai em ordem de data, com as entradas antes das
saídas no mesmo dia (o provento que cai no dia do aporte já estava no caixa), e
uma saída que passa do saldo o leva a zero em vez de a negativo. Isso só acontece
quando algo que sustentava um aporte é apagado depois — um provento, uma venda: o
excesso fica em `adjustment` e **não come as entradas futuras** (o provento
seguinte vale inteiro).

**Dinheiro de fora.** O aporte tem um valor (`amount`) e uma parte que veio do caixa
(`cash_used`, no máximo o saldo e o `amount`). A diferença é dinheiro novo da pessoa
e **não passa** pelo caixa: não entra, não sai, e o saldo não muda. Aportar mais do
que o caixa, ou aportar sem incluí-lo, é sempre válido; é a pessoa quem decide, na
tela, quanto do caixa entra no aporte ("Incluir" / "Tirar").

**O caixa começa em `cash_since`.** O que entrou antes dessa data (Brasília) não é
caixa: já foi gasto, ou nunca foi do Prumo — sem isso, todo dividendo recebido na
vida da pessoa viraria "caixa" no primeiro dia, e importar anos de extrato da B3
faria o mesmo. Quem já existia começa no dia **seguinte** ao do último aporte (a
regra de "proventos desde o último aporte" que o Prumo mostrava) ou, sem aporte,
hoje; quem chega depois, no dia em que chega. A data é `users.cash_since`
(migração 0012, que também cria `contributions.cash_used`, `0` nos aportes
antigos).

### `GET /api/v1/cash`

```json
{
  "balance": 812.0,
  "since": "2026-10-01",
  "income": { "amount": 612.0, "count": 3 },
  "sales": { "amount": 300.0, "count": 1 },
  "used": 100.0,
  "adjustment": 0.0,
  "recent": [
    { "date": "2026-11-20", "kind": "sale", "label": "ITSA4", "amount": 300.0 },
    { "date": "2026-11-14", "kind": "income", "label": "KNRI11", "amount": 12.0 },
    { "date": "2026-11-02", "kind": "contribution", "label": "Aporte", "amount": 100.0 }
## Objetivos

Metas com prazo: um patrimônio total (`net_worth`) ou um valor numa classe
(`class_value`, com `asset_class`). Ficam na tabela `objectives`, a mesma da
meta de renda mensal (`/income/goal`, acima), que **não** aparece aqui: criar
`monthly_income` por estas rotas é `422`, e o `id` dela dá `404` em
`PATCH`/`DELETE`. O nome **não** é `goals`: no Prumo, `goals` são as metas por
classe (`PUT /goals`).

Toda rota filtra pela pessoa: o `id` de outra pessoa é `404`.

### `GET /api/v1/objectives`

```json
{
  "defaults": { "annual_return_percent": 9.8, "return_source": "performance" },
  "objectives": [
    {
      "id": "…",
      "kind": "class_value",
      "asset_class": "br_reits",
      "target": 100000.0,
      "monthly_contribution": null,
      "expected_return": null,
      "current": 41200.0,
      "percent": 41.2,
      "status": "active",
      "achieved_at": null,
      "created_at": "2026-11-25T12:00:00Z",
      "assumptions": {
        "monthly_contribution": 750.0,
        "contribution_source": "ledger",
        "annual_return_percent": 9.8,
        "return_source": "performance"
      },
      "projection": { "state": "eta", "months": 52, "month": "2031-03" }
    }
  ]
}
```

- `balance = income.amount + sales.amount − used + adjustment`, nunca negativo.
- `income` e `sales`: o que entrou desde `since` e em quantos pagamentos ou
  vendas — é "de onde veio", que a tela mostra num detalhe ("De onde vem esse
  caixa").
- `recent`: até 30 movimentos, do mais novo ao mais velho (no mesmo dia, o que
  entra antes do que sai). `amount` é sempre positivo; `kind` diz o sentido
  (`income` e `sale` entram, `contribution` sai).
- Registrar o aporte com `cash_used` ([`POST /contributions`](#post-apiv1contributions))
  baixa o saldo na hora; editar ou apagar um provento ou um lançamento o refaz.
Do mais antigo para o mais novo. Cada leitura recalcula o progresso; se o valor
chegou à meta, grava `achieved_at` (uma vez só) e o objetivo passa a
`status: "achieved"`, com `assumptions: null` e `projection.state: "reached"`.
**Meta atingida não volta a ser ativa** se o patrimônio cair depois; só trocar o
valor da meta apaga a data (meta nova, conquista nova). A data é gravada por
quem primeiro vir a meta atingida: a **tarefa diária de desempenho** apura todos
os objetivos abertos de todas as pessoas (patrimônio, classe e a meta de renda
mensal, que só conquista com proventos recebidos), com o lock de job e um
`update ... where achieved_at is null`, então duas réplicas gravam uma data só e
reexecutar não muda nada; as leituras de `GET /objectives`, do Aportar e da tela
Renda fazem a mesma apuração na hora, sem esperar o dia seguinte. A data pode
ter, portanto, até um dia de atraso em relação ao momento em que o valor passou
da meta.

- `current`: o patrimônio total (`total_value` do `GET /portfolio`) ou o valor da
  classe.
- `percent`: de 0 a 100 (passou da meta, 100).
- `monthly_contribution` e `expected_return` são **o que a pessoa informou**
  (`null` = padrão); o que a conta usou está em `assumptions`.
- `defaults`: a rentabilidade que vale para quem não informou a sua.

#### A conta e as premissas

A data estimada é uma simulação mês a mês (`domain/objectives.rs`):
`saldo[m] = saldo[m-1] × (1 + anual)^(1/12) + aporte`, a partir do valor de hoje.
O primeiro mês em que o saldo chega à meta é a conclusão, contado do mês de hoje.
**A resposta sempre traz as duas premissas**, e a tela as mostra: a data nunca
vem sem a conta.

Aporte mensal (`assumptions.contribution_source`):

- `informed`: o `monthly_contribution` do objetivo.
- `ledger`: a média das **compras** (`buy`) do livro nos últimos 12 meses, em
  reais (valor mais taxas, no câmbio do lançamento), dividida pelos meses desde
  a primeira compra (meses de 30 dias, para cima, de 1 a 12): quem começou há
  quatro meses é medido por quatro. Num objetivo de classe só entram as compras
  daquela classe. Saldo inicial, bonificação e ajuste não são aporte; venda e
  resgate não abatem.

Rentabilidade anual (`assumptions.return_source`, `annual_return_percent` em %):

- `informed`: o `expected_return` do objetivo (de −50 a 100).
- `performance`: a rentabilidade da carteira em Desempenho (cotas/TWR), da
  janela de 12 meses, anualizada; só com pelo menos 180 dias de série. É bruta
  (antes do IR) e é da carteira inteira, também no objetivo de classe.
- `cdi_net`: sem série suficiente, o CDI de hoje (taxa diária mais recente do
  banco, composta por 252 dias úteis) menos 15% de IR (a menor alíquota de renda
  fixa).
- `fallback`: sem série nem CDI no banco, 8% ao ano fixos.

`projection.state`:

| `state` | Quando | `months` / `month` |
|---|---|---|
| `reached` | o valor já está na meta | `null` |
| `eta` | chega em até 100 anos | meses (≥ 1) e `AAAA-MM` |
| `unreachable` | sem aporte e sem rentabilidade positiva | `null` |
| `beyond` | chega, mas depois de 100 anos | `null` |

### `POST /api/v1/objectives`

```json
{ "kind": "class_value", "asset_class": "br_reits", "target": 100000,
  "monthly_contribution": 1500, "expected_return": 9.5 }
```

`kind` é `net_worth` ou `class_value`. `asset_class` é obrigatório no
`class_value` e proibido no `net_worth`. `target` é maior que zero (duas casas);
`monthly_contribution` (≥ 0) e `expected_return` (% ao ano, de −50 a 100) são
opcionais. `201` com o objetivo, já avaliado. Erros: `422`.

### `PATCH /api/v1/objectives/{id}`

Qualquer subconjunto de `target`, `monthly_contribution` e `expected_return`;
`null` nos dois últimos volta ao padrão. O tipo e a classe não mudam (campo
desconhecido é `422`): apague e crie outro. Trocar o valor de `target` apaga o
`achieved_at`. `200` com o objetivo; `404` se não existe, é de outra pessoa ou é
a meta de renda.

### `DELETE /api/v1/objectives/{id}`

`204`; `404` como acima.

### `POST /api/v1/objectives/impact`

O que um aporte faz com os objetivos **ativos**; a tela Aportar chama depois
do plano, com o que o plano compra, somado por classe:

```json
{ "items": [{ "asset_class": "br_reits", "value": 1000 }, { "asset_class": "br_stocks", "value": 500 }] }
```

```json
{
  "objectives": [
    {
      "id": "…", "kind": "net_worth", "asset_class": null,
      "target": 1000000.0, "current": 250000.0, "after": 251500.0,
      "percent_before": 25.0, "percent_after": 25.15,
      "assumptions": { "…": "como em GET /objectives" },
      "projection": { "state": "eta", "months": 51, "month": "2031-02" }
    }
  ]
}
```

`after` é `current` mais o que o aporte põe no objetivo (todos os itens no
patrimônio; só os da classe no objetivo de classe). `projection` conta a partir
de `after`, com as mesmas premissas de `GET /objectives`. Objetivo concluído (ou
que já está na meta) não entra. `items` aceita até 200 itens e valor ≥ 0; sem
itens, `after = current`. Não grava nada.

## Impostos

Estimativa do IR sobre bolsa e cripto a partir do livro e dos proventos. A
regra (por ano-calendário, com a fonte de cada número), a ligação com o livro e
o que fica fora estão em [`docs/ir.md`](ir.md). Tudo pelo dono.

**Instrumento fiscal** (`tax_instrument` do ativo): `stock`, `etf`, `bdr`,
`real_estate_fund`, `agribusiness_fund`, `crypto` ou `unsupported` (renda fixa,
Tesouro, ETF de renda fixa, exterior fora de BDR: fora da conta). O padrão
(`tax_instrument_default`) sai da classe, de `listed_on_b3` e do nome: o ETF de
bitcoin da B3 é `etf`; o de IMA-B, `unsupported`.

Ano sem regra cadastrada responde `422 tax_year_not_covered`.

### `GET /api/v1/tax/months?year=2026`

Os 12 meses do ano (sem `year`, o de hoje). Apura desde o primeiro ano com
regra (ou desde o ano do saldo informado), então dezembro passa o prejuízo para
janeiro do ano seguinte.

```json
{
  "year": 2026,
  "since_year": 2025,
  "covered_years": [2025, 2026],
  "today": "2026-11-25",
  "months": [
    {
      "year": 2026,
      "month": 5,
      "sales": [
        { "instrument": "stock", "common_sales": 24000.0, "day_trade_sales": 0.0, "common_result": 3000.0, "day_trade_result": 0.0 }
      ],
      "exemptions": [
        { "id": "stock_sales", "sales": 24000.0, "limit": 20000.0, "remaining": 0.0, "exempt": false, "exempt_gain": 0.0 },
        { "id": "crypto_sales", "sales": 0.0, "limit": 35000.0, "remaining": 35000.0, "exempt": true, "exempt_gain": 0.0 }
      ],
      "pools": [
        { "pool": "common", "result": 3000.0, "loss_before": 0.0, "loss_used": 0.0, "base": 3000.0, "rate": 0.15, "tax": 450.0, "loss_after": 0.0 }
      ],
      "capital_gain": { "gains": 0.0, "losses": 0.0, "exempt": true, "base": 0.0, "tax": 0.0 },
      "withholding": { "withheld": 0.0, "carried_in": 0.0, "used": 0.0, "carried_out": 0.0 },
      "darfs": [
        {
          "code": "6015", "regime": "exchange_net_gain", "tax": 450.0, "pending_before": 0.0,
          "total": 450.0, "payable": true, "amount": 450.0, "due_date": "2026-06-30",
          "pending_after": 0.0,
          "paid": { "amount": 450.0, "paid_at": "2026-06-20T12:00:00Z", "same_amount": true }
        },
        { "code": "4600", "regime": "capital_gain", "tax": 0.0, "pending_before": 0.0, "total": 0.0, "payable": false, "amount": 0.0, "due_date": "2026-06-30", "pending_after": 0.0, "paid": null }
      ],
      "notices": [],
      "doubts": []
    }
  ],
  "closing": {
    "losses": { "common": 0.0, "day_trade": 0.0, "real_estate_fund": 100.0, "agribusiness_fund": 0.0 },
    "pending_darf": { "exchange_net_gain": 0.0, "capital_gain": 0.0 }
  },
  "skipped": [
    { "subject": "KNRI11", "date": "2026-03-10", "reason": "amortization", "amount": 10.0 }
  ]
}
```

- `sales` traz todo instrumento (com zero); `pools`, os quatro grupos de
  compensação (`common`, `day_trade`, `real_estate_fund`, `agribusiness_fund`).
- `darfs`: um por código (`6015` bolsa, `4600` cripto). `payable` diz se há
  DARF a pagar no mês (R$ 10 ou mais, já com o acumulado de meses abaixo do
  mínimo); `pending_after` é o que passa para o mês seguinte. `paid` é a marca
  da pessoa, com o valor de quando marcou; `same_amount: false` diz que o livro
  mudou depois.
- `doubts`: os pontos de `docs/ir.md` § 5 que mexem num número do mês
  (`day_trade_in_stock_exemption`, `agribusiness_separate_from_real_estate`,
  `crypto_per_sale`, `year_end`).
- `skipped`: o que ficou fora da conta. `reason`: `unsupported_instrument`,
  `unknown_opening_cost`, `redemption`, `amortization`, `adjustment` (entrou
  como no livro, com aviso), `adjustment_closes_position`,
  `event_without_position`, `conflicting_instruments`. `date` nulo = o ativo
  inteiro.

### `GET /api/v1/tax/irpf/{year}`

A declaração do ano-calendário.

```json
{
  "year": 2025,
  "since_year": 2025,
  "covered_years": [2025, 2026],
  "assets": [
    {
      "symbol": "WEGE3", "instrument": "stock", "quantity": 100.0, "total_cost": 2563.13,
      "average_price": 25.63125, "group": "03", "code": "01",
      "label": "Acoes (inclusive as listadas em bolsa)", "code_source": "…"
    }
  ],
  "exempt_income": [
    { "group": "isentos", "code": "09", "label": "Lucros e dividendos recebidos", "code_source": "…",
      "gross": 50.0, "withheld": 0.0, "net": 50.0, "by_payer": [{ "symbol": "WEGE3", "gross": 50.0, "withheld": 0.0 }] }
  ],
  "exclusive_income": [
    { "group": "exclusiva", "code": "10", "label": "Juros sobre capital proprio", "code_source": "…",
      "gross": 100.0, "withheld": 15.0, "net": 85.0, "by_payer": [] }
  ],
  "year_end": { "losses": { "real_estate_fund": 100.0 }, "pending_darf": {} },
  "withholding_left": 0.0,
  "darf_total": 3816.59,
  "doubts": ["irpf_codes", "interest_on_equity_net"],
  "skipped": [{ "subject": "KNRI11", "date": "2025-03-10", "reason": "income_amortization", "amount": 10.0 }]
}
```

- `assets`: Bens e Direitos, posição em 31/12 pelo **custo de aquisição** (não
  o valor de mercado).
- `exempt_income`: dividendos (`09`), FII e Fiagro (`99`), lucro isento de
  ações (`20`) e de cripto (`05`). `exclusive_income`: JCP (`10`).
- Proventos: os recebidos até hoje; estimativa (`source = estimate`) não
  entra. `skipped` soma os do ano que ficam fora: `income_amortization`,
  `income_other`, `income_from_abroad` (dividendo de BDR),
  `income_out_of_scope`.
- `doubts`: `irpf_codes`, `withholding_leftover`, `dividend_withholding`,
  `interest_on_equity_net`.

### `POST /api/v1/tax/darfs/{year}/{month}/paid`

```json
{ "code": "6015" }
```

Marca o DARF do mês como pago, com o valor de agora (marcar de novo atualiza o
valor). Só um DARF que o Prumo apurou a pagar (`payable`); senão `422
darf_not_due`. `code` fora de `6015`/`4600`: `422`; mês fora de 1–12: `404`.
Responde `200` com o DARF (o objeto de `darfs`, com `paid`).

`DELETE /api/v1/tax/darfs/{year}/{month}/paid?code=6015` tira a marca: `204`,
ou `404` se não havia.

### Saldo de antes do Prumo: `/api/v1/tax/carryforward`

O prejuízo a compensar e o imposto abaixo do mínimo ainda não pago, como
estavam em 1º de janeiro de `year` — o primeiro ano que o Prumo apura.

`GET` (sem saldo informado: `informed: false`, zeros e o primeiro ano com
regra):

```json
{
  "year": 2025,
  "informed": true,
  "covered_years": [2025, 2026],
  "common_loss": 1000.0,
  "day_trade_loss": 0.0,
  "real_estate_fund_loss": 0.0,
  "agribusiness_fund_loss": 0.0,
  "pending_exchange_darf": 0.0,
  "pending_capital_gain_darf": 0.0
}
```

`PUT` troca o saldo inteiro: `year` (com regra) e os seis valores (ausente =
zero), em centavos e não negativos. Responde `200` com o `GET`.

## Alertas

Avisos fora do app, pelo Telegram, num resumo diário (como funciona, como criar
o bot e o que cada tipo avisa: [alertas.md](alertas.md)). Tudo pelo dono. Sem
`TELEGRAM_BOT_TOKEN` no servidor, a funcionalidade está desligada e nada
disto dá erro: `GET /alerts` responde com `telegram.configured: false`.

### `GET /api/v1/alerts`

```json
{
  "telegram": {
    "configured": true,
    "bot_username": "prumo_exemplo_bot",
    "linked": true,
    "linked_at": "2026-11-20T12:00:00Z"
  },
  "send_at": "08:00",
  "hide_values": false,
  "types": [
    { "kind": "income_tomorrow", "enabled": true, "available": true,
      "threshold": null, "default_threshold": null, "unit": null },
    { "kind": "class_drift", "enabled": true, "available": true,
      "threshold": 5.0, "default_threshold": 5.0, "unit": "percentage_points" },
    { "kind": "price_below_ceiling", "enabled": true, "available": true,
      "threshold": null, "default_threshold": null, "unit": null },
    { "kind": "objective_achieved", "enabled": true, "available": true,
      "threshold": null, "default_threshold": null, "unit": null },
    { "kind": "tax_exemption", "enabled": true, "available": true,
      "threshold": 80.0, "default_threshold": 80.0, "unit": "percent_of_limit" },
    { "kind": "health_warning", "enabled": true, "available": true,
      "threshold": null, "default_threshold": null, "unit": null }
  ]
}
```

- `telegram.configured`: o servidor tem o token do bot. `bot_username` (sem o
  `@`) é `null` sem token ou se o Telegram não respondeu.
- `send_at`: `HH:MM`, horário de Brasília. Padrão `08:00`.
- `hide_values`: o resumo vai sem valores em reais, só nomes e percentuais.
- `types`: sempre os seis, nesta ordem. `available: false` é um tipo que o
  servidor não pode apurar agora: hoje, só `tax_exemption`, quando o ano de hoje
  não tem regra de IR cadastrada (vem `enabled: false` e não liga).
  `unit` diz em que o limite é medido: `percentage_points` (p.p. da carteira) ou
  `percent_of_limit` (% do limite mensal); `null` nos que não têm limite.

### `PUT /api/v1/alerts`

Muda o que vier; o que ficar de fora fica como está. Ao menos um campo.

```json
{
  "send_at": "07:30",
  "hide_values": true,
  "types": {
    "class_drift": { "threshold": 7.5 },
    "health_warning": { "enabled": false }
  }
}
```

Devolve o mesmo corpo do `GET`. `422` para: hora fora de `HH:MM` (`00:00` a
`23:59`), tipo desconhecido, tipo sem `enabled` nem `threshold`, limite fora de
0–100 (exclusive o 0, até duas casas), limite num tipo que não tem, ligar um
tipo com `available: false`, campo desconhecido. Ligar um tipo, ou mudar o
limite dele, faz a próxima apuração só guardar o que já vale, sem avisar.

### `POST /api/v1/alerts/telegram`

Gera o código de vinculação (`201`):

```json
{
  "code": "K7M2QX9PTA",
  "command": "/start K7M2QX9PTA",
  "bot_username": "prumo_exemplo_bot",
  "expires_at": "2026-11-25T15:10:00Z"
}
```

A pessoa manda `command` ao bot (ou abre `https://t.me/<bot_username>?start=<code>`).
O código vale 10 minutos e uma vez; um novo invalida o anterior. O servidor lê
a mensagem por `getUpdates` e liga o `chat_id` à pessoa: a tela confere o
estado com `GET /alerts` até `telegram.linked` virar `true`. `422` se o
servidor não tem o token.

### `DELETE /api/v1/alerts/telegram`

Desvincula o chat (`204`, também quando não havia). O mesmo vale se a pessoa
mandar `/stop` ao bot, ou se o Telegram recusar um envio (bot bloqueado).

### `POST /api/v1/alerts/test`

"Enviar teste agora": manda ao chat vinculado uma mensagem curta, sem dado da
carteira, e responde `200` com `{ "sent": true }`.

| Status | Quando |
|---|---|
| `422` | sem token no servidor, ou nenhum chat vinculado |
| `429` (`too_many_requests`) | outro teste há menos de 10 segundos |
| `502` (`telegram_unavailable`) | o Telegram não aceitou a mensagem; o motivo cru vai para o log, não para a resposta |

## Fundamentos

Os indicadores da CVM de cada ativo da carteira: companhias pela DFP e pelo
ITR, FIIs pelos informes mensal e trimestral. Fontes, contas da CVM, fórmulas e
limites em [`fundamentos.md`](fundamentos.md). A tela lê o que a tarefa diária
gravou: nenhuma requisição espera a CVM.

**Medida** (`Figure`): sempre `{ "value": número | null, "gap": motivo | null }`,
exatamente um dos dois. Razões vêm como fração (`0.15` é 15%); valores em reais
(não em milhares). Os motivos (`gap`) e os de ativo sem fundamentos (`reason`)
estão em [`fundamentos.md`](fundamentos.md#motivos-gap).

### `GET /api/v1/fundamentals`

Na ordem da carteira; ativo sem símbolo (renda fixa por valor) fica fora.
Números fictícios:

```json
{
  "assets": [
    {
      "asset_id": "6f1c…",
      "symbol": "WEGE3",
      "kind": "company",
      "company": {
        "cvm_code": 99901,
        "name": "CIA FICTICIA S.A.",
        "template": "commercial",
        "judicial_recovery": false,
        "issuer_status": "FASE OPERACIONAL",
        "registration_status": "ATIVO",
        "latest_filing": { "kind": "quarterly", "reference_date": "2026-09-30", "version": 1, "scope": "consolidated", "column": "current" },
        "synced_at": "2026-11-26T15:00:00Z",
        "trailing": {
          "period_start": "2025-10-01",
          "period_end": "2026-09-30",
          "template": "commercial",
          "sources": [
            { "kind": "annual", "reference_date": "2025-12-31", "version": 1, "scope": "consolidated", "column": "current" },
            { "kind": "quarterly", "reference_date": "2026-09-30", "version": 1, "scope": "consolidated", "column": "current" },
            { "kind": "quarterly", "reference_date": "2026-09-30", "version": 1, "scope": "consolidated", "column": "previous" }
          ],
          "revenue": { "value": 1500000000.0, "gap": null },
          "net_income": { "value": 180000000.0, "gap": null },
          "equity": { "value": 1200000000.0, "gap": null },
          "equity_controlling": { "value": 1100000000.0, "gap": null },
          "gross_debt": { "value": 400000000.0, "gap": null },
          "cash": { "value": 100000000.0, "gap": null },
          "net_debt": { "value": 300000000.0, "gap": null },
          "ebit": { "value": 250000000.0, "gap": null },
          "depreciation_amortization": { "value": 50000000.0, "gap": null },
          "ebitda": { "value": 300000000.0, "gap": null },
          "operating_cash_flow": { "value": 200000000.0, "gap": null },
          "roe": { "value": 0.163636, "gap": null },
          "net_margin": { "value": 0.12, "gap": null },
          "ebitda_margin": { "value": 0.2, "gap": null },
          "net_debt_to_ebitda": { "value": 1.0, "gap": null },
          "net_debt_to_equity": { "value": 0.25, "gap": null }
        },
        "years": [
          { "year": 2021, "figures": null },
          { "year": 2022, "figures": { "…": "o mesmo formato de trailing" } }
        ],
        "profitable_years": { "positive": 4, "known": 4, "window": 5, "gap": null },
        "positive_operating_cash_years": { "positive": 4, "known": 4, "window": 5, "gap": null },
        "revenue_cagr": { "value": null, "gap": "insufficient_history" },
        "earnings_cagr": { "value": null, "gap": "insufficient_history" },
        "shares": 1000000000.0,
        "earnings_per_share": { "value": 0.18, "gap": null },
        "book_value_per_share": { "value": 1.1, "gap": null }
      },
      "fund": null,
      "reason": null
    },
    {
      "asset_id": "9a2e…",
      "symbol": "FICT11",
      "kind": "fund",
      "company": null,
      "fund": {
        "cnpj": "00.000.000/0001-91",
        "name": "FUNDO FICTICIO FII",
        "reference_date": "2026-10-01",
        "net_assets": { "value": 1000000000.0, "gap": null },
        "shares": { "value": 10000000.0, "gap": null },
        "nav_per_share": { "value": 100.0, "gap": null },
        "price": 90.0,
        "price_to_book": { "value": 0.9, "gap": null },
        "vacancy": { "value": 0.15, "gap": null },
        "vacancy_reference_date": "2026-09-30",
        "synced_at": "2026-11-26T15:00:00Z"
      },
      "reason": null
    },
    { "asset_id": "c3d4…", "symbol": "BOVA11", "kind": "unsupported", "company": null, "fund": null, "reason": "no_cvm_code" }
  ]
}
```

- `kind`: `company`, `fund` ou `unsupported` (com `reason`).
- `template`: `commercial`, `bank`, `insurer` ou `unknown`. Em banco e
  seguradora, receita, EBIT, dívida, caixa, EBITDA, margens e caixa operacional
  vêm com `gap: "financial_template"`.
- `trailing`: os últimos 12 meses (exercício + acumulado do ITR − acumulado do
  ano anterior) ou, sem trimestre depois do exercício, o próprio exercício; as
  datas e `sources` dizem qual. `null` sem nenhuma DFP.
- `years`: os cinco exercícios da janela, do mais velho ao mais novo;
  `figures: null` é ano sem balanço.
- `shares`: total de ações (ON + PN) da página do emissor na B3, com as em
  tesouraria; `earnings_per_share` (LPA) e `book_value_per_share` (VPA) usam os
  12 meses de `trailing`.
- `price` e `price_to_book` do FII usam a cotação gravada no Prumo; a tela
  recalcula o P/VP com o preço ao vivo da carteira.
- `synced_at`: a última vez que o Prumo conferiu a CVM com sucesso (empresas:
  os arquivos de DFP; FIIs: o informe mensal).

## Logos dos ativos

Cada ativo da B3 mostra o logo da empresa ou do fundo. O navegador **nunca** fala
com a brapi (nem com `icons.brapi.dev`): o servidor busca o logo uma vez, saneia
o SVG, guarda no banco e o serve do próprio domínio do Prumo. Como funciona, a
escolha de segurança e a nota sobre a licença da brapi: [icones.md](icones.md).

Sem `BRAPI_TOKEN` no servidor, a busca está desligada: o que já estiver guardado
continua sendo servido e nada disto dá erro. As duas rotas pedem a mesma
autenticação do resto da API e **não** passam pela regra `Cache-Control:
no-store` (o logo tem o próprio cache).

### `GET /api/v1/icons`

Quais símbolos têm logo guardado, sem os bytes. A tela faz esta consulta uma vez
e só pede a imagem do que está na lista (em vez de bater em 404 a cada linha).

```json
{ "icons": [{ "symbol": "ITSA4", "version": "6ae93b13c1d04f5e9a1b7c2d3e4f5a6b" }] }
```

`version` é o `ETag` do logo (sem aspas): a tela o põe na URL como `?v=` e o
navegador busca a imagem de novo quando o logo muda. `Cache-Control: no-store`.

### `GET /api/v1/icons/{symbol}`

O logo do símbolo (`ITSA4`, `KNRI11`; maiúsculas ou minúsculas, até 12 letras e
dígitos), do cache do banco. A chave do Tesouro (`TD-…`), cripto de verdade e
ativo dos EUA não têm logo: a tela usa o ícone próprio ou as iniciais.

| Status | Quando |
|---|---|
| `200` | há logo: corpo `image/svg+xml` já saneado |
| `304` | o `If-None-Match` cobre o `ETag` (aceita `*`, lista e `W/"…"`) |
| `404` | sem logo guardado, símbolo inválido ou logo recusado no saneamento; `Cache-Control: no-store` (ele pode chegar a qualquer momento) |
| `401` | sem credencial |

Cabeçalhos do `200` e do `304`:

| Cabeçalho | Valor |
|---|---|
| `ETag` | `"<md5 do SVG>"` |
| `Cache-Control` | `private, max-age=86400` |
| `Content-Security-Policy` | `default-src 'none'; style-src 'unsafe-inline'; sandbox` |
| `X-Content-Type-Options` | `nosniff` |
| `Content-Disposition` | `inline` |
| `Cross-Origin-Resource-Policy` | `same-origin` |

A CSP da página (`img-src 'self' data:`) não muda: a imagem é do mesmo domínio.

## Operacional (porta interna 9090)

`GET /healthz` (processo de pé), `GET /readyz` (`select 1` no Postgres),
`GET /metrics` (Prometheus). Na porta pública (8080) as três respondem 404.
