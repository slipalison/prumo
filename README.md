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
- **Teto de preço e limite por ativo** — guarda-corpos opcionais seus: acima
  do teto que você define, o ativo fica fora do aporte; com o limite, nenhum
  ativo passa de X% da carteira, e o excedente vai para os outros da classe.
  Preço médio não entra na conta: o Prumo não compra mais só porque caiu.
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
- **Importar da B3** — suba as planilhas da Área do Investidor (Negociação,
  Movimentação, Proventos), confira o que é novo, o que já está na carteira e o
  que conflita, ajuste a classe e o saldo inicial de antes de 2019 e importe:
  compras, vendas, desdobros, bonificações, Tesouro e proventos entram de uma
  vez, e importar de novo não duplica. Ver
  [`docs/importacao-b3.md`](docs/importacao-b3.md).
- **Livro de lançamentos** — compra, venda, desdobramento, grupamento,
  bonificação, amortização, resgate, saldo inicial e ajuste, com data. A
  quantidade é a soma do livro; dele saem o preço médio (regra da Receita,
  com taxas), o lucro realizado e a rentabilidade de cada ativo, em reais
  mesmo no que é cotado em dólar. O "Já comprei" grava as compras sozinho.
- **Renda** — dividendos, JCP e rendimentos de FII que você recebeu ou vai
  receber, mês a mês, contra uma meta de renda mensal. O aporte mostra quanto de
  renda estimada ele soma. A renda "da carteira de hoje" é **estimativa**
  (histórico por cota do Yahoo, bruto, buscado em segundo plano: nenhuma tela
  espera por ele): o que você lança, e depois o que importa da B3, vale mais. O
  provento lançado sem quantidade vale as cotas que o livro tinha na **data com**,
  não as de hoje. **Proventos automáticos:** o Prumo lê na B3 os dividendos, JCP
  e rendimentos anunciados para os seus ativos e calcula o que você recebe com
  as cotas do livro na data com (selo "automático (B3)"); o que você lançou à
  mão ou importou sempre vence, e bonificação, desdobramento e grupamento
  aparecem como pendência para lançar. Liga e desliga em Ajustes. Cada pagador, o ativo e a carteira mostram o **yield on cost**
  (renda de 12 meses ÷ o que você pagou; sem preço médio, a tela pede o preço
  médio em vez de inventar).
- **Caixa** — o que os proventos recebidos e as vendas deixaram e você ainda não
  aportou. O campo do aporte oferece "+ R$ X em caixa — incluir?", com um detalhe
  de onde o saldo veio; registrar o aporte baixa o que foi usado, e o resto é
  dinheiro de fora. O motor não muda, só o valor que você digita.
- **Lucro total** — na Carteira: ganho de capital (realizado + não realizado)
  mais os proventos recebidos. O que falta saber (preço médio, cotação) deixa o
  número marcado como parcial, sem estimativa.
  renda estimada ele soma e oferece incluir os proventos recebidos desde o
  último aporte. A renda "da carteira de hoje" é **estimativa** (histórico por
  cota do Yahoo, bruto, buscado em segundo plano: nenhuma tela espera por ele): o
  que você lança, e depois o que importa da B3, vale mais.
- **Objetivos** — metas com prazo, em Ajustes: um patrimônio total ou um valor
  numa classe ("R$ 1 milhão", "R$ 100 mil em FIIs"). Cada uma mostra o
  progresso e o mês estimado de conclusão, e **a conta fica sempre à vista**:
  o aporte mensal (a média real das suas compras nos últimos 12 meses, ou o que
  você informar) e a rentabilidade (a da sua carteira em Desempenho, o CDI de
  hoje menos IR ou o que você informar). O Aportar diz quanto cada aporte
  aproxima você da meta. É estimativa, não promessa. A meta de renda mensal
  continua em Renda.
- **Preço ao vivo** — patrimônio e ativos mudam na tela sem recarregar, pelo
  streamer do Yahoo: EUA, câmbio e cripto em segundos; B3 com os 15 minutos
  de atraso do dado gratuito. Tudo convertido para reais.
- **Análise da carteira** — o que olhar além da meta por classe, em quatro
  dimensões: diversificação (concentração, setores, classes, exterior, renda
  fixa), dividendos (yield, quem paga, renda que cresce e cai todo mês),
  rentabilidade (contra CDI, Ibovespa e IPCA em 12 meses e 5 anos, anos no azul,
  volatilidade, perda desde a compra) e qualidade dos ativos (aguarda os dados de
  fundamentos da CVM). Cada regra é sua: liga, desliga e escolhe o limite, e a
  pontuação de uma dimensão é só o percentual das regras com dado que estão ok,
  com o número exato. Regra sem dado não é falha. Traz ainda o perfil de risco
  (carteira e metas), os destaques de 12 meses e referências de preço públicas
  (Bazin, e Graham quando houver fundamentos) para comparar com o seu teto, sem
  recomendar nada. O setor vem da B3 (ações) e da CVM (FIIs). Detalhes em
  [`docs/analise.md`](docs/analise.md) e [`docs/saude.md`](docs/saude.md).
- **Fundamentos** — na folha de cada ação e FII da B3, os números da CVM com a
  fonte e a data: receita, lucro, ROE, margem, dívida líquida sobre EBITDA e
  sobre o patrimônio, LPA e VPA, anos com lucro e com caixa operacional
  positivo, crescimento de receita e lucro em 5 anos e o aviso de recuperação
  judicial; nos FIIs, valor patrimonial por cota, P/VP com o preço ao vivo e a
  vacância física declarada. Banco e seguradora mostram só o que o plano de
  contas deles tem. Tudo dos dados abertos da CVM, conferidos uma vez por dia;
  número que falta aparece como "—" com o motivo, nunca como zero. Detalhes em
  [`docs/fundamentos.md`](docs/fundamentos.md).
- **Alertas** — um resumo por dia no Telegram, só quando há algo a dizer:
  provento que cai amanhã, classe fora da meta, ativo de volta ao teto de
  preço, objetivo atingido, aviso novo de saúde e isentômetro de ações do mês. Cada tipo liga e desliga, a
  hora é sua, e dá para mandar sem valores em reais. Você cria o bot no
  @BotFather e o vincula por um código, sem procurar chat id. Passo a passo em
  [`docs/alertas.md`](docs/alertas.md).
- **Logos dos ativos** — cada ação e FII da B3 mostra o logo da empresa ou do
  fundo; Tesouro, cripto e o que não tem logo usam ícone próprio ou as iniciais
  do ticker na cor da classe. A fonte é a [brapi](https://brapi.dev): o
  servidor busca cada logo **uma vez** (o plano gratuito dá 15 mil pedidos por
  mês), saneia o SVG e o serve do domínio do Prumo; o navegador não fala com
  terceiros. Para ligar: crie uma conta gratuita na brapi e guarde a chave em
  `BRAPI_TOKEN`; sem ela, tudo segue com as iniciais. Detalhes, segurança e a
  nota sobre a licença em [`docs/icones.md`](docs/icones.md).
- **Desempenho** — patrimônio no tempo (aplicado × ganho, e os proventos) e
  rentabilidade pelo método de cotas, com os proventos como retorno, contra
  CDI, IPCA e Ibovespa, com a tabela mês a mês. A série começa no primeiro
  lançamento do livro: importou a B3, o histórico aparece desde a primeira
  negociação, com desdobramentos e o Tesouro Direto. Lançamento antigo editado
  ou apagado refaz a série dali em diante, em segundo plano. Detalhes em
  [`docs/desempenho.md`](docs/desempenho.md).
- **Impostos** — em Ajustes: o isentômetro do mês (vendas de ações contra os
  R$ 20 mil), o DARF de cada mês (6015 e 4600) com vencimento e a marca de
  pago, e a declaração do ano para copiar (Bens e Direitos pelo custo em 31/12,
  isentos e JCP), tudo saído do livro e dos proventos. Cada ativo tem o
  instrumento fiscal (ação, ETF, BDR, FII, Fiagro, cripto ou fora), com padrão
  pela classe e pelo nome e editável na folha. O que não foi calculado aparece
  com o motivo, e as dúvidas jurídicas como "ponto a confirmar com contador".
  É **estimativa**: não substitui o contador. Regras por ano (2025 e 2026),
  com a fonte, em [`docs/ir.md`](docs/ir.md).

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
- [`docs/analise.md`](docs/analise.md) — a Análise: regras, fórmulas, limiares e o que ela não faz
- [`docs/saude.md`](docs/saude.md) — as sete regras originais (hoje dentro da Análise), fontes de setor e limitações
- [`docs/importacao-b3.md`](docs/importacao-b3.md) — planilhas da B3: formato, mapeamento e limites
- [`docs/design.md`](docs/design.md) — interface, identidade e movimento
- [`docs/ir.md`](docs/ir.md) — regras de IR por ano, com fonte, e o que não é calculado
- [`docs/alertas.md`](docs/alertas.md) — alertas no Telegram: criar o bot, vincular, tipos, privacidade
- [`docs/icones.md`](docs/icones.md) — logos dos ativos: fonte (brapi), como ligar, segurança do SVG e licença
- [`docs/fundamentos.md`](docs/fundamentos.md) — fundamentos da CVM: fontes, contas, fórmulas, limites e o contrato
- [`docs/desempenho.md`](docs/desempenho.md) — rentabilidade por cotas, índices e fontes
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
| `DATABASE_MAX_CONNECTIONS` | `4` | teto do pool por réplica (1–100); com o limite de 20 do papel no Postgres, cabe o canário de 4 pods |
| `AUTH_MODE` | — | obrigatória: `cloudflare-access` ou `dev` |
| `CF_ACCESS_TEAM_DOMAIN` | — | obrigatória no Access (`<team>.cloudflareaccess.com`) |
| `CF_ACCESS_AUD` | — | obrigatória no Access (AUD da aplicação) |
| `DEV_USER_EMAIL` | `dev@prumo.local` | só no modo `dev` |
| `PORT` / `OPERATIONAL_PORT` | `8080` / `9090` | pública / sondas e métricas |
| `STATIC_DIR` | `static` | onde está o build do frontend |
| `QUOTES_ENABLED` | `true` | `false` desliga o provedor de cotações |
| `QUOTES_STREAM` | `true` | `false` desliga o preço ao vivo (fica o `chart` a cada `QUOTES_REFRESH_SECS`) |
| `QUOTES_REFRESH_SECS` | `900` | intervalo da atualização automática (mín. 60) |
| `SECTORS_ENABLED` | `true` | `false` desliga a busca de setor (B3 e CVM, a cada 30 dias) |
| `FUNDAMENTALS_ENABLED` | `true` | `false` desliga a busca de fundamentos (CVM e página do emissor na B3, conferidas uma vez por dia) |
| `SNAPSHOT_AT` | `19:00` | horário de Brasília da foto diária do desempenho (`HH:MM`), ou `off` |
| `BRAPI_TOKEN` | — | segredo: chave da brapi (conta gratuita em brapi.dev), só do servidor. Sem ela, os logos dos ativos ficam desligados (iniciais no lugar); precisa de saída HTTPS para `brapi.dev` e `icons.brapi.dev` |
| `TELEGRAM_BOT_TOKEN` | — | segredo: token do bot do Telegram (criado por você no @BotFather). Sem ele, os alertas ficam desligados e a tela diz isso; precisa de saída HTTPS para `api.telegram.org` |

O binário recusa subir com configuração pela metade.

## Testes

```bash
cd backend && cargo test          # unitários + integração (sobe Postgres 17 no Docker)
cd frontend && npm test           # vitest com cobertura
```

## Licença

MIT.
