# Prumo

**Mantém a carteira no eixo.** Você diz quanto vai aportar; o Prumo diz o que
comprar para a carteira ficar o mais perto possível das suas metas.

- **Metas por classe** — Ações Nacionais, FIIs, Ações Internacionais, REITs,
  Cripto, Renda Fixa e Renda Fixa Internacional, somando 100%.
- **Nota por checklist** — cada ativo responde sim/não a critérios que você
  edita; cada sim soma 1, cada não tira 1. A nota é o peso do ativo dentro da
  classe.
- **O aporte** — proporcional ao que falta em cada classe, nunca além da meta,
  em cotas inteiras, com o troco reaproveitado. O que não tem destino aparece
  com o motivo. Detalhes em [`docs/alocacao.md`](docs/alocacao.md).
- **A lista do que você quer ter** — ativo cadastrado com quantidade 0 já
  entra na divisão pela nota: é assim que uma classe vazia passa a receber.
- **Negociado na B3** — um ETF de bitcoin conta em Cripto, um BDR ou ETF de
  S&P 500 em Ações Internacionais, cotados na B3 em cota inteira.
- **Atual × ideal por ativo** — a Carteira mostra, em cada ativo, quanto ele é
  da classe hoje e quanto deveria ser (a fatia que o motor usa), com um selo
  "abaixo do ideal" e a contagem no topo.
- **Ocultar valores** — o olho na navegação mascara R$, quantidades e totais em
  todas as telas, inclusive para o leitor de tela; a escolha fica salva no
  navegador.
- **Tesouro Direto** — cadastre o título pelo nome ("Tesouro IPCA+ 2035") e a
  quantidade, com fração (0,35). O preço de resgate vem sozinho da tabela
  pública do Tesouro Transparente, uma vez por dia; o aporte compra em
  centésimos de título. CDB e LCI seguem por valor. Detalhes em
  [`docs/tesouro.md`](docs/tesouro.md).
- **Preço ao vivo** — patrimônio e ativos mudam na tela sem recarregar, pelo
  streamer do Yahoo: EUA, câmbio e cripto em segundos; B3 com os 15 minutos
  de atraso do dado gratuito. Tudo convertido para reais.

No ar em <https://prumo.alisonamorim.com>, atrás do Cloudflare Access.

## Como é feito

| | |
|---|---|
| Backend | Rust 1.98, Axum 0.8, sqlx 0.9, Postgres 17 |
| Frontend | React 19, Vite, TypeScript, Motion |
| Imagem | uma só: o binário serve a API e o PWA (`Dockerfile`) |
| Esteira | [`slipalison/github-workflows`](https://github.com/slipalison/github-workflows) (`.github/workflows/ci.yml`) |
| Deploy | [`slipalison/homelab-gitops`](https://github.com/slipalison/homelab-gitops), `apps/prumo/` — chart `app`, ArgoCD, canary |

Documentos:

- [`docs/api.md`](docs/api.md) — contrato da API
- [`docs/alocacao.md`](docs/alocacao.md) — o algoritmo do aporte
- [`docs/tesouro.md`](docs/tesouro.md) — o Tesouro Direto: fonte, modelo e limites
- [`docs/design.md`](docs/design.md) — interface, identidade e movimento
- [`docs/ir.md`](docs/ir.md) — regras de IR por ano, com fonte, e o que não é calculado
- [`deploy/README.md`](deploy/README.md) — como o app chega ao cluster

## Rodar na máquina

Pré-requisitos: Rust 1.98, Node 24, Docker.

```bash
# Postgres descartável
docker run -d --name prumo-db -e POSTGRES_PASSWORD=prumo -p 55432:5432 postgres:17-alpine

# Backend (porta 8080; sondas na 9090)
cd backend
DATABASE_URL=postgres://postgres:prumo@localhost:55432/postgres AUTH_MODE=dev cargo run

# Frontend (porta 5173, com proxy de /api para a 8080)
cd frontend
npm ci --ignore-scripts
npm run dev
```

`AUTH_MODE=dev` aceita toda requisição como `DEV_USER_EMAIL` (padrão
`dev@prumo.local`). Em produção é `cloudflare-access`.

### Variáveis do backend

| Variável | Padrão | |
|---|---|---|
| `DATABASE_URL` | — | obrigatória |
| `AUTH_MODE` | — | obrigatória: `cloudflare-access` ou `dev` |
| `CF_ACCESS_TEAM_DOMAIN` | — | obrigatória no Access (`<team>.cloudflareaccess.com`) |
| `CF_ACCESS_AUD` | — | obrigatória no Access (AUD da aplicação) |
| `DEV_USER_EMAIL` | `dev@prumo.local` | só no modo `dev` |
| `PORT` / `OPERATIONAL_PORT` | `8080` / `9090` | pública / sondas e métricas |
| `STATIC_DIR` | `static` | onde está o build do frontend |
| `QUOTES_ENABLED` | `true` | `false` desliga o provedor de cotações |
| `QUOTES_STREAM` | `true` | `false` desliga o preço ao vivo (fica o `chart` a cada `QUOTES_REFRESH_SECS`) |
| `QUOTES_REFRESH_SECS` | `900` | intervalo da atualização automática (mín. 60) |

O binário recusa subir com configuração pela metade.

## Testes

```bash
cd backend && cargo test          # unitários + integração (sobe Postgres 17 no Docker)
cd frontend && npm test           # vitest com cobertura
```

## Licença

MIT.
