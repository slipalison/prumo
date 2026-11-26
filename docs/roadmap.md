# Roteiro: do aporte ao acompanhamento

O Prumo responde "onde ponho o próximo real?". Este roteiro dá a ele a
memória que falta — lançamentos com data e preço, proventos, série
histórica — e o que sai dela: preço médio, rentabilidade, renda passiva,
evolução do patrimônio, metas com prazo e IR. A regra de ouro não muda: o
motor decide pela meta e pela nota e nunca passa da meta.

Princípios:

- Toda informação vem de fonte pública e gratuita ou da própria pessoa (B3,
  CVM, BCB, Tesouro, Yahoo). Nada de raspar site de terceiros.
- Exemplos, testes e fixtures com carteira fictícia: o repositório é público.
- Cada fase só termina no ar (ver [§ 5](#5-roadmap-em-fases)).

---

## 3. Funcionalidades, por prioridade (valor × esforço)

Escala de esforço: P = até 2 dias · M = 3–6 dias · G = 7+ dias, sempre com
testes, docs, deploy e conferência em produção.

### 3.1 [P0 · esforço P] % ideal e "abaixo do ideal" por ativo na Carteira

- **O que é:** mostrar em cada ativo a % atual na classe, a % ideal
  (nota/Σ notas) e um selo "abaixo do ideal". No topo, "N ativos abaixo do
  ideal".
- **Por que importa:** o Prumo já calcula isso no motor e esconde o
  número. Dá transparência ao critério.
- **Dados:** nenhum novo.
- **Mudanças:**
  - `PortfolioView` ganha `ideal_percent` e `gap` por ativo (mesma fórmula
    do passo 4 de `docs/alocacao.md`).
  - `AssetDto` ganha `ideal_class_percent`.
  - A Carteira mostra uma barra fina atual × ideal na linha do ativo.
- **Testes:** unidade da fórmula (soma 100% na classe; ativo de nota 0 tem
  ideal 0) e teste de tela.
- **Riscos:** nenhum relevante.

### 3.2 [P0 · esforço P] Ocultar valores

- **O que é:** um botão de olho que mascara R$ e quantidades e fica salvo no
  `localStorage`.
- **Mudanças:** só no front; o `CountUp` e o `formatBRL` passam a respeitar a
  máscara.
- **Riscos:** leitor de tela. Mascarar o texto lido também, para não vazar.

### 3.3 [P1 · esforço G] Livro de lançamentos + preço médio + rentabilidade por ativo (fundação)

- **O que é:** toda mudança de quantidade vira um lançamento datado: compra,
  venda, desdobramento, grupamento, bonificação, amortização, resgate e saldo
  inicial. Do livro saem a quantidade, o **preço médio** (regra da Receita:
  custo médio ponderado com taxas; venda não muda o PM), o **lucro realizado**
  e a **rentabilidade** de cada posição.
- **Por que importa:** é a base de rentabilidade, lucro, IR, evolução e
  yield on cost.
- **Dados:** o que a pessoa digita, o "Já comprei" do plano e a importação da
  B3 (3.4).
- **Mudanças:**
  - Migração `transactions`:
    - `id`, `user_id`, `asset_id` (FK, `on delete set null`), `symbol` e
      `asset_class` (foto do momento);
    - `kind` (`buy|sell|split|reverse_split|bonus|amortization|redemption|opening`);
    - `trade_date date`, `quantity numeric(28,10)`, `unit_price numeric(24,8)`
      (nulo em `opening` sem custo conhecido), `fees numeric(20,2) default 0`;
    - `currency`, `fx_rate` (para ativo em dólar);
    - `source` (`manual|plan|b3_import`) e `external_ref`, com índice único
      por pessoa para a importação ser idempotente;
    - `contribution_id` (liga ao aporte do Prumo).
  - **Migração de dados:**
    - cada ativo vira um lançamento `opening` com a quantidade atual e custo
      desconhecido (a pessoa informa o PM ou importa da B3);
    - cada `contribution_item` vira um `buy` com `contribution_id`.
  - **`assets.quantity`:** vira cache mantido na mesma transação que grava o
    lançamento (ou visão materializada). A regra é uma fonte da verdade só:
    o livro.
  - **Domínio** (`domain/ledger.rs`, puro): posição (qtd, PM, custo, lucro
    realizado) a partir da lista de lançamentos, com eventos societários.
    Testes de propriedade com proptest: qtd nunca negativa, PM invariante em
    venda, desdobramento preserva o custo total.
  - **API:**
    - `GET/POST /api/v1/transactions` (filtro por ativo e período);
    - `PATCH/DELETE /api/v1/transactions/{id}`;
    - em `AssetDto`: `average_price`, `cost_basis`, `unrealized_gain`,
      `realized_gain` e `return_percent`.
  - **Telas:**
    - a folha do ativo ganha a aba "Lançamentos" (lista e adicionar);
    - a linha da Carteira mostra PM e rentabilidade;
    - o patrimônio mostra "Lucro total = ganho de capital + proventos"
      quando 3.5 existir.
- **Riscos:**
  - Eventos societários (desdobramento, bonificação com custo atribuído,
    troca de ticker, incorporação). Começar com os quatro comuns e um tipo
    "ajuste" manual.
  - Venda a descoberto não existe (regra: venda não passa da posição).

### 3.4 [P1 · esforço M] Importar o extrato da B3 (Área do Investidor)

- **O que é:** subir as planilhas que a própria B3 exporta na Área do
  Investidor (investidor.b3.com.br):
  - **Negociação:** data, compra/venda, mercado, código, quantidade, preço e
    valor.
  - **Movimentação:** liquidação, dividendo, JCP, rendimento, desdobro,
    bonificação, leilão de fração, resgate do Tesouro etc.
  - **Proventos** recebidos e **provisionados** (anunciados, com data de
    pagamento).
- **Por que importa:** é ter o histórico sem contrato. A API oficial da B3
  para dados do investidor é só para instituições parceiras credenciadas. Um
  app pessoal não acessa, e raspar a sessão da B3 não é aceitável. A planilha
  é dado da própria pessoa, baixado por ela.
- **Dados:** os arquivos `.xlsx` da Área do Investidor. Limites:
  - o histórico começa em ~2019, na migração do CEI;
  - corretagem e emolumentos não vêm, e o PM fica um pouco abaixo do fiscal
    (deixar a pessoa ajustar a taxa).
- **Mudanças:**
  - Crate `calamine` para ler xlsx no backend.
  - `POST /api/v1/imports/b3` (multipart) com **pré-visualização**: lista o
    que entra, o que já existe pelo `external_ref` e o que não foi
    entendido, e só grava no `POST .../{id}/commit`.
  - Mapeamento do "Produto" da B3 para ativo e classe usando as regras de
    classificação de `quotes.rs`; ativo novo nasce com quantidade do livro.
  - Tela: "Importar da B3" em Ajustes, com passo a passo e prints da Área
    do Investidor.
- **Testes:** planilhas de exemplo **fictícias** no formato real (cabeçalhos
  e colunas iguais), importação idempotente e linha desconhecida reportada.
- **Riscos:**
  - A B3 muda o layout da planilha. Mitigar com detecção por cabeçalho e
    mensagem clara.
  - Os dados são sensíveis: os arquivos nunca vão para log nem para disco
    fora do banco.

### 3.5 [P1 · esforço M] Proventos e meta de renda passiva

- **O que é:**
  - proventos recebidos e a receber (dividendos, JCP líquido, rendimentos
    de FII, amortização);
  - histórico mensal e anual, distribuição por pagador e média de 12 meses;
  - **yield on cost** (renda de 12 m / custo, depende de 3.3);
  - **meta de renda mensal**.
- **Por que importa:** para quem investe em dividendos e FIIs é o painel mais
  olhado, e a meta de renda é a pergunta-mãe do investidor de longo prazo.
- **Dados, fontes legítimas e gratuitas:**
  - **Recebidos e provisionados:** a importação da B3 (3.4). É a melhor
    fonte, porque vem com valor líquido e data de pagamento.
  - **Histórico por cota para projetar:** Yahoo `chart` com
    `events=div,split` (data ex e valor bruto; sem data de pagamento, e JCP
    misturado com dividendo nas ações da B3). Serve para estimativa, não
    para extrato.
  - **FIIs:** dados abertos da CVM, informe mensal de FII em
    `dados.cvm.gov.br` (rendimentos distribuídos, segmento e mandato).
- **Mudanças:**
  - Migração `income_events`:
    - `id`, `user_id`, `asset_id`, `kind`, `ex_date` (data com),
      `pay_date`;
    - `amount_per_unit`, `quantity`, `gross`, `withholding`, `net`;
    - `source`, `external_ref` único.
  - Status recebido/a receber calculado pela `pay_date`.
  - API:
    - `GET /api/v1/income?from&to&status&asset`;
    - `GET /api/v1/income/summary` (12 m, média e por pagador);
    - `POST` manual.
  - Meta de renda: tabela `objectives` (ver 3.7).
  - Tela nova **Renda**: total de 12 m contra a meta, barras mensais
    (recebido/a receber), lista dos próximos pagamentos, ano × mês.
- **Riscos:** projeção a partir do histórico do Yahoo pode errar (JCP, data).
  Chamar de "estimativa" e preferir sempre o dado importado.

### 3.6 [P2 · esforço G] Evolução do patrimônio e rentabilidade vs índices

- **O que é:**
  - patrimônio mês a mês (aplicado × ganho);
  - rentabilidade pelo **método de cotas (TWR)**, que neutraliza o efeito
    dos aportes, como em fundo;
  - opcionalmente a TIR (MWR);
  - comparação com **CDI, IPCA e Ibovespa** (e IFIX/IVVB11 se houver
    fonte);
  - tabela mês × ano.
- **Por que importa:** responde "estou ganhando do CDI?". Sem isso, a
  pessoa não sabe se a estratégia funciona.
- **Dados, fontes legítimas e gratuitas:**
  - **CDI diário:** BCB/SGS série 12. **IPCA mensal:** série 433. **Selic:**
    série 11. Exemplo:
    `api.bcb.gov.br/dados/serie/bcdata.sgs.12/dados?formato=json&dataInicial=…`.
  - **Ibovespa:** Yahoo `^BVSP`. **IVVB11 e ações:** Yahoo `chart` diário
    ou mensal (fechamento ajustado para eventos).
  - **IFIX:** a B3 publica a série histórica dos índices em página pública
    (download). Se não der automático, deixar de fora no começo.
  - **Tesouro Direto:** Tesouro Transparente, CSV público de preços e taxas
    históricos (`PrecoTaxaTesouroDireto.csv`).
- **Mudanças:**
  - Tabela `price_history (symbol, date, close)` como cache das séries.
  - Tabela `index_history (code, date, value)`.
  - Tabela `portfolio_snapshots (user_id, date, market_value, net_flow,
    quota_value, quotas)`.
  - Tarefa diária no fechamento, mais reconstrução retroativa a partir do
    livro: quando a pessoa importa a B3, o histórico aparece.
  - API:
    - `GET /api/v1/history/net-worth?period=12m&class=`;
    - `GET /api/v1/performance?period=…&benchmarks=cdi,ipca,ibov`.
  - Tela nova **Desempenho**: patrimônio no tempo, linha de rentabilidade
    contra os índices, mês × ano. Gráficos em SVG próprio com Motion, como a
    rosca e as barras atuais.
- **Riscos:**
  - Volume de séries históricas: cachear e baixar só o que a carteira usa.
  - Fechamento ajustado contra o não ajustado (escolher um e escrever no
    `docs/`).
  - Ativo sem histórico (renda fixa por valor): tratar pela variação
    informada ou pelo indexador.

### 3.7 [P2 · esforço M] Metas com prazo (patrimônio, renda, classe)

- **O que é:** objetivos como "R$ 1 milhão", "R$ 3 mil/mês de renda" ou
  "R$ 100 mil em FIIs", com progresso, aporte mensal médio (real, do livro)
  e **conclusão estimada**. A meta atingida fica registrada com a data.
- **Por que importa:** dá horizonte ao aporte. "Faltam 14 aportes como o de
  hoje."
- **Dados:** livro (aporte médio), snapshots (rentabilidade média) e
  proventos (renda).
- **Mudanças:**
  - Migração `objectives`: `id`, `user_id`, `kind`
    (`net_worth|monthly_income|class_value`), `asset_class` (opcional),
    `target`, `monthly_contribution` (opcional; o padrão é a média real),
    `achieved_at`.
  - O nome **não** pode ser `goals`: no Prumo, `goals` são as metas por
    classe.
  - API CRUD.
  - Card na tela Aportar, abaixo do resultado: "com este aporte, você chega
    a X% da meta".
- **Riscos:** a projeção depende de premissa de retorno. Mostrar a premissa
  e deixar editar.

### 3.8 [P2 · esforço M] Tesouro Direto com preço automático

- **O que é:** título do Tesouro cadastrado por nome (ex.: "Tesouro IPCA+
  2035") e quantidade fracionária, cotado pelo preço de recompra do dia, no
  lugar do "valor digitado".
- **Por que importa:** hoje a renda fixa do Prumo fica parada até a pessoa
  atualizar à mão, e o Tesouro é grande parte de muita carteira.
- **Dados:** CSV público do Tesouro Transparente (preço e taxa diários de
  cada título).
- **Mudanças:**
  - Novo `Market::Treasury` (unidade fracionária, cotação do CSV
    atualizada 1×/dia).
  - Busca de símbolo em `/symbols` passa a incluir os títulos vigentes.
  - Renda fixa por valor continua existindo para CDB e LCI.
- **Riscos:** o CSV muda de endereço de vez em quando; o horário de
  atualização fica fora do pregão.

### 3.9 [P3 · esforço M] Saúde da carteira (regras próprias) + setor/segmento

> Entregue, e depois evoluída para a **Análise da carteira** (dimensões de
> diversificação, dividendos, rentabilidade e qualidade): ver
> [analise.md](analise.md). As regras abaixo seguem dentro dela.

- **O que é:** um painel de **avisos** (sem nota 0–100) com regras próprias e
  **configuráveis**. Exemplos:
  - nenhum ativo acima de X% da carteira;
  - os 5 maiores abaixo de Y%;
  - nenhum setor acima de Z%;
  - renda em todo mês;
  - nenhum pagador acima de W% da renda.

  Alertas de fundamentos ficam **fora** (ver 3.12).
- **Por que importa:** pega o que a meta por classe não vê: concentração
  dentro da classe e por setor.
- **Dados:**
  - setor e segmento das ações pela **Classificação Setorial da B3**
    (planilha pública);
  - segmento e tipo dos FIIs pelo informe mensal da CVM;
  - ações dos EUA pelo código SIC da SEC (EDGAR, API pública).
- **Mudanças:**
  - Colunas `sector` e `segment` em `assets` (ou tabela de referência por
    símbolo, atualizada mensalmente).
  - `domain/health.rs` puro, com regras parametrizadas.
  - `GET /api/v1/health`.
  - Rosca por setor dentro de cada classe na Carteira.
- **Riscos:** regra demais vira ruído. Começar com 4–5 e deixar desligar.

### 3.10 [P3 · esforço G] IR: isentômetro, apuração mensal, DARF e IRPF

- **O que é:**
  - **Isentômetro:** vendas de ações no mês contra o limite de isenção.
  - **Apuração mensal** por categoria: ações em operação comum, day trade,
    FII, ETF/BDR e cripto (com o limite próprio de vendas mensais).
  - Compensação de prejuízo acumulado por categoria.
  - DARF (código e vencimento).
  - **IRPF:** Bens e Direitos com custo em 31/12, rendimentos isentos
    (dividendos e rendimentos de FII de pessoa física), tributação exclusiva
    (JCP) e operações por mês.
- **Por que importa:** é obrigação legal e dá trabalho todo ano.
- **Dados:** o livro (3.3) com vendas e taxas, os proventos (3.5) e as
  regras da Receita do ano.
- **Mudanças:**
  - `domain/tax.rs` puro, com a **tabela de regras versionada por ano**
    (alíquota, limites de isenção, quais categorias têm isenção).
  - API:
    - `GET /api/v1/tax/months?year=`;
    - `GET /api/v1/tax/irpf/{year}`;
    - `POST /api/v1/tax/darfs/{year}/{month}/paid`;
    - `GET`/`PUT /api/v1/tax/carryforward` (saldo de antes do Prumo).
  - Tela **Impostos**.
- **Riscos (os maiores do plano):**
  - Legislação muda (houve propostas recentes de unificar alíquotas): as
    regras não podem ficar embutidas no código.
  - Erro custa multa.
  - Exige revisão das regras com fonte oficial (Receita, Perguntas e
    Respostas do IRPF) a cada ano e aviso de responsabilidade.

### 3.11 [P3 · esforço P–M] Alertas

- **O que é:** avisos fora do app.
- **Exemplos:**
  - provento cai amanhã;
  - classe ficou X p.p. fora da meta;
  - ativo passou do teto de preço da pessoa;
  - meta atingida;
  - isentômetro perto do limite.
- **Dados:** os de cima.
- **Canal:** o homelab já entrega alertas no Telegram; um bot próprio do
  Prumo pode reaproveitar o caminho. Push de PWA também é opção.
- **Riscos:** spam. Resumo diário em vez de evento a evento.

### 3.12 Fora do escopo, por ora

- **Notas e indicadores de fundamentos automáticos** (ROE, dívida/EBITDA,
  margens). Dá para fazer com os **dados abertos da CVM** (DFP/ITR). Poderia
  **sugerir** respostas do checklist do Prumo, mas a pessoa sempre confirma. É
  esforço G+ e fica para depois do resto.
- **Várias carteiras:** pouco valor para uma pessoa só. Se vier, é coluna
  `portfolio_id` em tudo.

---

## 4. A junção: como o aporte do Prumo melhora com esses dados

A regra de ouro continua: **o motor decide pela meta e pela nota, e nunca passa
da meta**. Os dados novos entram como insumo ou como informação, sem virar
complicação.

1. **Caixa de proventos no aporte.** O campo "Quanto você vai aportar?"
   ganha uma sugestão do tipo "+ R$ 812 de proventos recebidos desde o último
   aporte (incluir?)".
   - Precisa de um saldo de caixa por pessoa (`cash_balance`), alimentado
     por provento líquido e venda e consumido pelo aporte registrado.
   - O motor não muda; só o valor de entrada.
2. **Renda projetada no resultado.** Cada item do plano mostra a renda
   estimada que ele soma, a partir da renda de 12 m por cota. O topo mostra
   "este aporte soma ~R$ 35/mês de renda" e "sua renda vai de R$ X para
   R$ Y".
3. **Preço médio e rentabilidade só como informação no plano.** Por exemplo:
   "você paga R$ 25,00; seu PM é R$ 22,10".
   - **Decisão explícita:** o motor **não** usa o PM para decidir. Comprar
     mais do que caiu porque caiu é viés de preço médio, não critério.
     Escrever isso em `docs/alocacao.md`.
4. **Teto de preço por ativo (critério da pessoa).** Campo opcional
   `max_price` no ativo.
   - Acima do teto, o ativo sai da divisão daquele aporte, com aviso no
     motivo: "XPTO3 acima do seu teto de R$ 30".
   - É indicador da própria pessoa, não um "preço justo" de terceiros.
   - No motor, é um filtro a mais no passo 2 (elegibilidade), com teste de
     propriedade.
5. **Limite de concentração (guarda-corpo opcional).** "Nenhum ativo acima de
   X% da carteira".
   - No passo 4 (dentro da classe), a fatia de um ativo é limitada pelo teto
     e o excedente vai para os outros da classe, como o troco já faz.
   - Atualizar `docs/alocacao.md` e o proptest.
6. **Metas com prazo usando o aporte real.** O resultado do plano mostra "com
   aportes assim, a meta de R$ 1 mi chega em mar/2031".
7. **Mais adiante: rebalanceamento com venda, consciente de IR.** Hoje o Prumo
   nunca vende, e isso deve continuar como padrão. Se um dia houver "rebalancear
   vendendo", a sugestão de venda respeita o limite de isenção do mês e
   prefere o lote com menor ganho.

O que **não** muda: a classe acima da meta continua sem receber (decisão já
registrada) e o fluxo continua de uma tela, digitar o valor e ver o plano.

---

## 5. Roadmap em fases

Toda fase só termina com:
- testes (unidade, propriedade quando há regra e integração com Postgres
  via testcontainers);
- front com teste e cobertura ≥ 80%;
- `docs/api.md` e `docs/alocacao.md` atualizados;
- commit Conventional em pt-BR e esteira verde;
- **deploy feito, `/api/v1/info` com o commit novo e a funcionalidade
  conferida em produção com os dados reais da pessoa** (sem gravar nada que
  ela não pediu).

| Fase | Escopo | Pronto quando | Estimativa |
|---|---|---|---|
| **F0** | 3.1 % ideal/abaixo do ideal + 3.2 ocultar valores | Carteira mostra atual × ideal por ativo; olho mascara tudo, inclusive para o leitor de tela; no ar | 1–2 dias |
| **F1** | 3.3 livro de lançamentos, preço médio, rentabilidade por ativo; "Já comprei" grava lançamentos; migração dos dados atuais (`opening` + `contribution_items`) | Toda quantidade bate com a soma do livro (teste e conferência em produção); PM e rentabilidade na Carteira; editar/apagar lançamento recalcula | 5–8 dias |
| **F2** | 3.4 importação da B3 (negociação, movimentação, proventos) com pré-visualização e idempotência | A pessoa importa os próprios arquivos e o histórico aparece; reimportar não duplica; linha não reconhecida é listada | 4–6 dias |
| **F3** | 3.5 proventos (recebidos/a receber, histórico, distribuição, YoC) + meta de renda + renda projetada no plano + caixa de proventos no aporte | Tela Renda com os dados importados; o plano mostra a renda somada; sugestão de caixa no campo do aporte | 4–6 dias |
| **F4** | 3.6 snapshots, preços históricos, índices BCB/Yahoo, TWR, gráfico contra CDI/IPCA/IBOV, mês × ano | Tela Desempenho; a rentabilidade bate com um cálculo de conferência (planilha) num caso real; série retroativa a partir do livro | 6–9 dias |
| **F5** | 3.7 metas com prazo + 3.8 Tesouro automático + teto de preço e limite de concentração no motor | Metas com ETA no Aportar; título do Tesouro cotado sozinho; teto e concentração com proptest e docs | 5–7 dias |
| **F6** | 3.9 saúde (regras próprias configuráveis) + setor/segmento | Painel de avisos com 4–5 regras ligáveis; rosca por setor | 4–6 dias |
| **F7** | 3.10 IR (isentômetro, apuração, compensação, DARF, IRPF) com regras versionadas por ano | Casos de teste montados a partir das Perguntas e Respostas da Receita; conferência com uma apuração conhecida da pessoa | 8–12 dias |
| **F8** | 3.11 alertas (Telegram ou push) | Resumo diário chega; dá para desligar cada tipo | 2–3 dias |

Ordem sugerida: F0 → F1 → F2 → F3 → F4 → F5 → F6 → F7 → F8. A F1 é
pré-requisito de quase tudo; a F2 deixa as seguintes úteis desde o primeiro dia
(sem digitar anos de histórico).

**Organização da interface:** hoje são Aportar,
Carteira, Metas e Critérios. A proposta fica assim:
- **Aportar**: igual, mais caixa de proventos, renda projetada e prazo das
  metas.
- **Carteira**: mais PM, rentabilidade, ideal e setor; na folha do ativo, a
  aba Lançamentos.
- **Renda**: nova.
- **Desempenho**: nova.
- **Ajustes**: Metas por classe, Critérios, Objetivos, Importar da B3 e
  Impostos.

Assim ficam 5 itens na navegação, e nenhum é tela de "painel pelo painel".

---

## 7. Fontes de dados — resumo

| Dado | Fonte gratuita e legítima | Observação |
|---|---|---|
| Lançamentos, proventos recebidos e provisionados, eventos | Área do Investidor da B3, exportação `.xlsx` feita pela pessoa | Histórico desde ~2019; sem taxas de corretagem |
| CDI, Selic, IPCA | BCB/SGS (séries 12, 11 e 433) | API JSON aberta |
| Ibovespa, ações, ETFs, BDR, EUA, cripto (histórico) | Yahoo `chart` (já usado pelo Prumo) | Não oficial; já é a fonte de cotação |
| Proventos por cota (histórico, para estimar) | Yahoo `chart` com `events=div,split` | Só data ex e valor bruto |
| FIIs: rendimentos, segmento, mandato | CVM dados abertos, informe mensal de FII | Mensal |
| Setor e segmento das ações | B3, Classificação Setorial (planilha pública) | Atualização periódica |
| Setor das empresas dos EUA | SEC EDGAR (código SIC) | API pública |
| Tesouro Direto (preço e taxa diários) | Tesouro Transparente, CSV público | Atualiza 1×/dia |
| Fundamentos (fase futura) | CVM dados abertos, DFP/ITR | Grande; só para sugerir respostas do checklist |
| Regras de IR | Receita Federal (legislação e Perguntas e Respostas do IRPF) | Versionar por ano |
