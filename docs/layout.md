# Layout: densidade, grade e movimento

O pedido: aproveitar a tela larga, rolar menos, entender mais rápido, menos
texto à vista (explicação sob demanda), minimalista sem perder o vidro e o
movimento, e animações mais rápidas. A referência é a **densidade** de um
painel de análise de carteira com muita coisa acima da dobra — cartões lado a
lado, tabelas compactas com estado —, nunca a identidade visual de terceiros.

Este documento é o plano. A **etapa 1** (feita) mediu as telas, escreveu este
plano e entregou a base comum: tokens, primitivas, toggletip de uso geral,
casca nova e o CSS repartido por área. A **etapa 2** redesenha as telas, em
quatro grupos que rodam em paralelo com arquivos disjuntos (§ 8).

## 1. Onde estamos (medida "antes")

Medido no modo de demonstração (§ 9) sobre a `main` em `ab25259`. "Telas" é
a altura da página dividida pela altura da janela: 1,0 cabe sem rolar.

| Tela | 1440×900 | 1920×1080 | iPad 1366×1024 | iPad 1024×1366 | Celular 390×844 |
|---|---|---|---|---|---|
| Aportar (vazio) | 1,03 | 1,02 | 1,02 | 1,02 | 1,00 |
| Aportar (plano calculado) | 2,20 | 1,83 | 2,11 | 1,71 | 4,14 |
| Carteira | 3,01 | 2,51 | 2,64 | 2,02 | 4,05 |
| Renda | 2,61 | 2,17 | 2,28 | 1,61 | 3,42 |
| Desempenho | 1,84 | 1,53 | 1,62 | 1,27 | 2,74 |
| **Análise** | **11,16** | **9,30** | **9,90** | **7,85** | **17,25** |
| Ajustes › Metas | 1,44 | 1,20 | 1,29 | 1,02 | 2,77 |
| Ajustes › Critérios | 1,03 | 1,02 | 1,02 | 1,02 | 2,13 |
| Ajustes › Objetivos (1 objetivo) | 1,03 | 1,02 | 1,02 | 1,02 | 1,36 |
| Ajustes › Análise | 4,70 | 3,92 | 4,16 | 3,69 | 8,78 |
| Ajustes › Importar da B3 (antes de ler) | 1,03 | 1,02 | 1,02 | 1,02 | 1,71 |
| Ajustes › Impostos | 4,30 | 3,58 | 3,82 | 3,10 | 6,38 |
| Ajustes › Alertas | 2,18 | 1,82 | 1,99 | 1,60 | 3,65 |
| Ajustes › Proventos automáticos | 1,03 | 1,02 | 1,02 | 1,02 | 1,91 |

Folhas (conteúdo ÷ altura da folha; a folha rola por dentro):

| Folha | Desktop (1440–1920) | Celular |
|---|---|---|
| Ativo (ação, com Lançamentos e Fundamentos) | 1,82–1,89 | 2,31 |
| Ativo (FII, checklist de 6 perguntas) | 2,31–2,40 | 2,89 |
| Checklist (aberta do aviso do plano) | 2,36–2,45 | 2,98 |
| Novo ativo, Registrar aporte, Lançar provento, Meta de renda, Objetivo, Teto | 1,00 | 1,00–1,18 |

### O que a medida mostrou

- **Coluna estreita numa tela larga.** O conteúdo parava em 1100 px e o trilho
  ocupava 252 px + 56 px de margem: em 1920 px, ~40% da largura era fundo. As
  telas são uma pilha de cartões de largura total, um embaixo do outro; nada
  fica lado a lado, então a altura cresce com tudo que entra.
- **Texto que explica à vista o tempo todo.** Cada tela abre com um parágrafo
  de 2 a 3 linhas; cada cartão tem uma frase de "como se calcula"; a Análise
  repete "O que fazer" e a conta em cada regra; Referências de preço abrem com
  três parágrafos de fórmula; Impostos tem um aviso de 3 linhas e duas listas
  fixas. Tudo isso é útil uma vez e ruído nas outras: vira ⓘ (§ 5).
- **Análise: 11 telas.** Cada regra é um bloco de 120–330 px (estado, frase,
  Agora/Limite grandes, barra, conta, "O que fazer", link), inclusive as que
  estão ok. 36 regras × ~150 px. As quatro dimensões só aparecem uma depois da
  outra; o resumo por dimensão (a pontuação) fica no topo de cada bloco, não
  junto.
- **Carteira: uma seção por classe, um ativo por bloco de 70 px** (ícone,
  nome, quantidade, valor, nota, barra do ideal embaixo), mais rosca por setor
  dentro da classe. A informação de ativo cabe numa linha de tabela de 40 px.
- **Renda e Desempenho:** gráficos com 300–380 px de altura e largura total;
  números-resumo em cartões separados com recheio grande.
- **Ajustes:** a lista de oito seções quebra em duas linhas de pílulas no topo;
  Ajustes › Análise repete o cartão-por-regra da Análise (4,7 telas).
- **Folhas:** coluna única de 620 px no desktop, com dicas de uma a três
  linhas sob cada campo.
- **Espaço vazio:** na tela de plano há ~180 px em branco antes de "Seus
  objetivos" (o espaço do botão flutuante "Já comprei"); a Carteira termina com
  "Sem meta e sem ativo" e "Histórico de aportes" em cartões inteiros.

### Movimento antes

Medido no código (molas convertidas em tempo de assentamento a 2%):

| Onde | Antes | Assenta em |
|---|---|---|
| Troca de tela (`Shell`, `AnimatePresence mode="wait"`) | sai 220 ms + entra 220 ms | **~440 ms** sem conteúdo |
| Contagem dos números (`CountUp`) | mola 70/18 | ~690 ms |
| Rosca (`Donut`) | mola 60/16 | ~660 ms |
| Barras de classe, limite, meta de renda, objetivos | mola 80/18 (+250 ms de atraso no pedaço dourado) | ~640–900 ms |
| Barras do mês a mês | mola 90/18 + 25 ms por barra + 200 ms | até ~1 s |
| Linha do gráfico de desempenho | mola 70/20 | ~720 ms |
| Cartões do plano | cascata de 60 ms, mola 260/24 | ~550 ms no último |
| Rosca antes → depois no plano | espera fixa de 650 ms | 650 ms + mola |
| Folha | mola 320/30 | ~300 ms |
| Hover e cor no CSS | 0,2–0,4 s soltos em cada arquivo | — |

## 2. Princípios

1. **Largura vira colunas, não espaço em branco.** O conteúdo vai até 1680 px;
   a partir de ~1000 px de conteúdo, as telas usam 2 ou 3 colunas.
2. **Acima da dobra fica o que responde a pergunta da tela** e o resumo de tudo
   o que está embaixo (números lado a lado, contagens, estado).
3. **Uma linha por item** nas listas longas (tabela compacta com estado). O
   detalhe de um item vai para a folha, para um ⓘ ou para um recolher.
4. **Explicação é sob demanda.** Texto que se lê uma vez vira ⓘ (§ 5). Fica à
   vista só o que muda a decisão: o número, o estado, a ação.
5. **Menos moldura.** Um título por bloco (sem eyebrow + título + subtítulo),
   recheio de 16 px, sem cartão dentro de cartão quando uma linha resolve.
6. **O vidro, a aurora, o prumo e o dourado ficam.** O minimalismo é de
   conteúdo, não de identidade.
7. **Rápido.** Nada que a pessoa espera passa de ~300 ms (§ 6).
8. **Nada se perde para quem usa leitor de tela ou teclado**: tabela é
   `<table>`, estado é texto, ⓘ tem nome, recolher é `<details>`.

## 3. Grade e casca

### Larguras

| Faixa da janela | Navegação | Margem lateral | Largura do conteúdo |
|---|---|---|---|
| < 960 px (celular, iPad em retrato estreito) | barra de abas embaixo | 16 px | toda |
| 960–1599 px (iPad deitado, notebook, 1440) | **trilho estreito, 84 px**: ícone com o nome embaixo | 16 px, vão de 20 px | toda (1440 → ~1300 px) |
| ≥ 1600 px (monitor largo) | trilho de 208 px, nome ao lado do ícone, e-mail e versão | 24 px, vão de 28 px | até **1680 px** (`--content-max`) |

- O trilho estreito devolve 168 px ao conteúdo em 1440 px e cabe no iPad
  deitado; o nome continua visível (11 px) sob cada ícone, e o olho e o tema
  ficam só com o ícone (o nome acessível não muda).
- Tela que ainda não foi redesenhada fica na coluna de antes (1100 px,
  `--content-legacy`). Tela redesenhada declara `<Page>` e ganha a largura
  larga (`.main:has(.page)`).
- O que é fixo e centrado no conteúdo (o "Já comprei") usa
  `--content-center-x`, que já desconta o trilho.

### Grade (`<Grid>`, `.grid`)

A grade responde à largura **do conteúdo** (container query em `.page`,
`.panel` e `.sheet__body`), não à da janela: a seção de Ajustes com lista
lateral, uma coluna de 1/3 ou uma folha larga mudam a largura disponível e a
grade acompanha sem media query por tela.

| `cols` | < 640 px de conteúdo | 640–999 px | ≥ 1000 px |
|---|---|---|---|
| `2` | 1 coluna | 2 | 2 |
| `3` | 1 | 2 | 3 |
| `4` | 1 | 2 | 4 |
| `main-aside` / `aside-main` | 1 | 1 | 2/3 + 1/3 |
| `auto` (`minWidth`, padrão 260 px) | quantas couberem | | |

`.span-2` ocupa duas colunas (a partir de 640 px) e `.span-full` a linha
inteira. `alignStart` alinha os cartões no topo em vez de esticar.

### Celular e iPad

- Celular continua celular: uma coluna, barra de abas, folha que sobe de
  baixo. O ganho no celular vem do texto que vira ⓘ, das tabelas compactas (que
  empilham abaixo de 560 px com o nome da coluna antes de cada valor) e dos
  recolheres — a meta é **metade da altura** de hoje nas telas longas.
- iPad em retrato (1024 px): trilho estreito e 2 colunas onde a grade pede.
  iPad deitado (1366 px): 3 colunas nas grades de 3.
- Alvos de toque seguem ≥ 44 px (o ⓘ tem 16 px à vista e 44 px de alvo).

## 4. Espaço e densidade

Tokens em `styles/tokens.css`:

- **Escala de 4 px:** `--space-1` (4) `-2` (8) `-3` (12) `-4` (16) `-5` (20)
  `-6` (24) `-8` (32). Tela redesenhada usa só estes passos.
- **Painel:** recheio `--pad-panel` 16 px (`--pad-panel-compact` 12 px); vão
  entre painéis `--gap-grid` 12 px; vão entre blocos da página
  `--gap-stack` 12 px. (Antes: 18–22 px de recheio e 18 px de vão.)
- **Linha de tabela** `--row-height` 40 px; com botão na linha, `--touch` 44 px.
- **Texto:** `--text-2xs` 0,7 rem, `-xs` 0,75, `-sm` 0,85, `-md` 0,92,
  `-lg` 1,05. Título de tela ~1,35–1,7 rem (antes até 2,3 rem); título de
  painel 1 rem.
- **Menos molduras e títulos:** o cabeçalho da tela é uma linha (título + ⓘ +
  ações); o eyebrow só onde ajuda a se localizar (Ajustes); painel tem um
  título, sem subtítulo; números dentro de painel ficam em fundo afundado, sem
  borda (`StatStrip inset`).

## 5. Tooltip, recolher ou folha

| Use | Quando | Exemplo |
|---|---|---|
| **ⓘ toggletip** (`InfoTip`, `Toggletip`) | Explicação de uma a três frases, que a pessoa lê uma vez: como se calcula, de onde vem, o que um termo quer dizer, a ressalva fixa. Nunca o único lugar de um número ou de uma ação. | "Renda dos últimos 12 meses ÷ o que você pagou", a fórmula do Bazin, "Hoje é parcial", o porquê de uma regra, a nota de que o índice saiu até tal data. |
| **Recolher** (`Disclosure`, `<details>`) | Conteúdo secundário com várias linhas ou valores: listas, a conta parcela a parcela, o que está ok. Fechado, a frase diz o que tem dentro e quantos. | "8 regras ok", "Como cada mês foi apurado", "De onde vem esse caixa", "O que o Prumo não calculou (3)". |
| **Folha** (`Sheet`) | Editar, confirmar, um formulário, ou o detalhe inteiro de um item (que tem as próprias seções). | Ativo, lançar provento, objetivo, definir teto. |
| **À vista** | O número que responde a pergunta, o estado (ok / ponto a ver / sem dado), a ação principal, e o aviso que muda a decisão (o de responsabilidade do imposto fica à vista, mas em uma linha, com o resto no ⓘ). | |

Regras do toggletip:

- O gatilho tem nome próprio, que já diz o que ele explica ("Como se calcula o
  yield on cost"); num chip, o nome leva o veredito ("Bazin R$ 9,50: não
  aprova").
- Abre no toque (iPad), no foco e no mouse; fecha com Esc, tocando fora ou
  saindo do foco. O texto entra numa região `role="status"`.
- Abre em cima (`placement="top"`, padrão) e vira para baixo se não couber;
  desloca-se para dentro da janela nas bordas. `align`: `center` (padrão),
  `start`, `end` ou `stretch` (largura do bloco posicionado mais próximo, para
  cartões que cortam o que passa da borda).
- Com o olho fechado, todo R$ dentro do balão passa por `useMoney()`, como no
  resto.

## 6. Movimento

Tokens em `lib/motion.ts` (motion) e `styles/tokens.css` (CSS), com um teste
que confere que os dois lados batem e que tudo é rápido:

| Token | Valor | Para |
|---|---|---|
| `DURATION.instant` / `--dur-instant` | 90 ms | pressão, saída de tela |
| `DURATION.fast` / `--dur-fast` | 140 ms | hover, cor, toggletip, recolher |
| `DURATION.base` / `--dur-base` | 200 ms | entrada de tela, folha, bloco |
| `DURATION.slow` / `--dur-slow` | 280 ms | barras, roscas, linhas |
| `SPRING.snappy` | 520/40 (~0,2 s) | alternâncias, indicador da navegação, nota que vira |
| `SPRING.enter` | 420/36 (~0,22 s) | folha, aviso, cartão que entra |
| `SPRING.value` | 190/27 (~0,3 s) | barras, roscas, contadores, linha do gráfico |
| `SPRING.magnet` | 320/24 | botão magnético |
| `STAGGER` | 30 ms | cascata de listas |
| `EASE_OUT` / `--ease-out` | (0,22, 1, 0,36, 1) | o que entra |
| `EASE_IN` / `--ease-in` | (0,4, 0, 1, 1) | o que sai |

Já aplicado na etapa 1 em todo o app: troca de tela 90 + 200 ms (antes 440),
contadores, roscas e barras ~0,3 s (antes 0,6–1 s), folha ~0,22 s, cascata do
plano 30 ms, rosca antes → depois em 400 ms (antes 650), todo `transition` do
CSS por token. Ficam de fora, por serem ambiente ou identidade: a aurora, o
prumo balançando enquanto calcula, o brilho do esqueleto, o pulso do "Ao vivo"
e o lampejo de 1,6 s do preço (que diz o sentido).

`prefers-reduced-motion`: `MotionConfig reducedMotion="user"` e a regra
global de `base.css` continuam valendo; tela nova não precisa fazer nada além
de usar os tokens.

## 7. As telas

Meta de altura em 1440×900, com a carteira fictícia da demonstração. "Sobe"
é o que passa a ficar acima da dobra; "ⓘ" é o texto que sai da tela para um
toggletip.

### 7.1 Aportar (grupo 1)

```
┌ Aportar ──────────────────────────────────────────────── [Já comprei] ┐
│┌ Quanto você vai aportar? ┐ ┌ Compre 4 ativos ─────────────────────────┐│
││ R$ 6.000,00              │ │ Investe R$ 5.947 · Troco R$ 52 · Renda   ││
││ + R$ 812 em caixa Incluir│ │ +~R$ 34/mês ⓘ                            ││
││ [500][1.000][2.000][5mil]│ │┌ITSA4──────┐┌BTLG11─────┐┌TRXF11────┐┌BTC─┐││
││ [Calcular]               │ ││R$ 2.721   ││R$ 756     ││R$ 914    ││... │││
││ ▸ De onde vem esse caixa │ ││243 × 11,20││8 × 94,55  ││17 × 53,81│└────┘││
│└──────────────────────────┘ ││✗Bazin ✓Gr.││✓Bazin ✓Gr.││✗Graham   │      ││
│                             │└───────────┘└───────────┘└──────────┘      ││
│                             └──────────────────────────────────────────┘│
│┌ Avisos (2) ─────────────────┐┌ Carteira depois ──┐┌ Seus objetivos ─────┐│
││⚠ Ações Int. sem ativo  + Adic││ rosca  barras     ││ 25,6% da meta ⓘ    ││
││⚠ HGLG11 acima do teto Abrir →││                   ││ fev/2031           ││
│└─────────────────────────────┘└───────────────────┘└────────────────────┘│
```

- Desktop: `Grid cols="aside-main"`: o campo à esquerda (1/3, compacto, o prumo
  pequeno), o resultado à direita (2/3). Antes do primeiro cálculo, o campo
  fica centrado como hoje.
- A manchete vira uma `StatStrip`: ativos, investido, troco e a renda que o
  aporte soma (o "estimativa" e a frase "sua renda vai de X para Y" no ⓘ).
- Cartões do plano em `Grid cols="auto" minWidth="200px"`, mais baixos: valor,
  "cotas × preço", a renda do cartão em uma linha curta; as referências sem teto
  continuam em chips (já com a explicação no ⓘ).
- Avisos num painel só, uma linha por aviso com a ação à direita.
- "A carteira depois do aporte" e "Seus objetivos" lado a lado; as premissas
  dos objetivos e "a data é uma estimativa…" vão para o ⓘ.
- "Já comprei": no desktop, no cabeçalho do resultado (sem o espaço vazio de
  hoje); no celular, continua flutuante.
- **Meta:** ≤ 1,2 tela com plano (antes 2,2); celular ≤ 2,5 (antes 4,1).

**Como ficou (grupo 1, medido em 1440×900 e demais larguras):**

- Com plano: 1,02 tela em 1440×900, 1920×1080 e iPad deitado; 1,02–1,05 no iPad em
  pé; celular 2,48 (antes 2,20 / 1,83 / 2,11 / 1,71 / 4,14). Tudo cabe acima da
  dobra no desktop; 1,0 é o piso da casca (a página nunca é menor que a janela).
- A região `Resultado do aporte` virou `display: contents` dentro da grade da
  tela: o painel "Compre N ativos" fica ao lado do campo e os painéis de avisos,
  carteira depois e objetivos ocupam a linha de baixo. Uma região só, porque
  testes de outros grupos (`privacy.test`, `IncomePrivacy.test`) procuram os
  avisos e a frase da renda dentro dela.
- **Desvio do plano:** a manchete não virou `StatStrip`. A frase "com R$ X · troco
  de R$ Y" e a frase da renda ("Este aporte soma ~R$ …/mês: sua renda vai de A
  para B") seguem escritas, porque `privacy.test.tsx` (congelado) e
  `IncomePrivacy.test.tsx` (grupo 3) as conferem por texto. Se quem integra
  atualizar esses dois testes, a faixa de números e o ⓘ da renda cabem sem custo.
- O "Já comprei" fica fora do painel de vidro (o `backdrop-filter` prenderia a doca
  fixa do celular dentro dele): posição absoluta no canto do painel no desktop,
  fixa no rodapé abaixo de 960 px.
- Em painel estreito (< 460 px) a rosca de antes/depois some: as barras já são
  `meter` com o antes e o depois por extenso. Cartões do plano: dois por linha
  no celular.
- Premissas dos objetivos no plano: ⓘ por objetivo; a frase da data passou a dizer
  "No ritmo das premissas" (valia "da conta abaixo", e abaixo deixou de haver conta).

### 7.2 Ajustes › Metas, Critérios e Objetivos (grupo 1)

- **Metas:** perfis prontos numa linha de botões segmentados (a descrição de
  cada perfil no ⓘ); os sete controles em `Grid cols={2}` dentro de um painel;
  à direita (`main-aside`) o anel de soma, a rosca, Equilibrar/Salvar e o
  **Limite por ativo** (o texto longo do limite no ⓘ). Meta ≤ 1,0 (antes 1,44).
- **Critérios:** o parágrafo de abertura vira ⓘ; as duas listas como hoje
  (já lado a lado). Meta ≤ 1,0 em 1440 e −30% no celular.
- **Objetivos:** cartões em `Grid cols="auto" minWidth="320px"`; as premissas
  ficam à vista (é a regra da tela: nenhuma data sem a conta), em fonte menor e
  uma linha cada. Meta: 3 objetivos em ≤ 1,0.

**Como ficou (grupo 1):** Metas, Critérios e Objetivos ficam em 1,01–1,02 tela
nas quatro larguras largas (o piso da casca) e, no celular, 2,01 / 1,84 / 1,10
(antes 2,77 / 2,13 / 1,36). Metas: os perfis prontos vão para o cabeçalho da
tela (botões segmentados, descrições no ⓘ); sliders em duas colunas; à direita
o anel de soma e a rosca lado a lado, e o Limite por ativo. Objetivos: o título
leva uma linha com o link da Renda e o resto no ⓘ; cartões em `Grid auto`
320 px. Medida com a casca de Ajustes antiga (a lista de seções ainda em duas
linhas de pílulas, do grupo 4).

### 7.3 Carteira e folhas do ativo (grupo 2)

```
┌ Carteira ─────────────────────────────────────────────────── [+ Ativo] ┐
│ Patrimônio R$ 120.132 ●Ao vivo │ Lucro total +R$ 3.120 ⓘ │ 2 abaixo do  │
│ (StatStrip)                    │                         │ ideal │ Análise: 13 →│
│┌ Ações Nacionais 23,3% · meta 30% ▬▬▬▬▬▬|▬ ─────────┐┌ Por classe ─────┐│
││ Ativo   Qtd·Preço·PM      Valor   Rent.  % classe/ideal  Nota││ rosca + legenda ││
││ ITSA4   2.500·11,20·10,00 28.000  +12,0%  ▬▬| 100/100   3   │└─────────────────┘│
│└───────────────────────────────────────────────────────┘┌ Setores (Ações)─┐│
│┌ Fundos Imobiliários 5,4% · meta 10% ──────────────────┐│ barras de setor ││
││ KNRI11 …  ↓ abaixo do ideal                            │└─────────────────┘│
││ HGLG11 …                                               │┌ Histórico ──────┐│
│└───────────────────────────────────────────────────────┘│ (recolhido)     ││
│ Sem meta e sem ativo: [+ REITs] [+ RF Internacional]     └─────────────────┘│
```

- Topo: `StatStrip` com patrimônio (e o selo ao vivo), lucro total (as três
  parcelas no ⓘ, "parcial" escrito), "N abaixo do ideal" e o resumo da Análise
  ("13 pontos a ver", link).
- `Grid cols="main-aside"`: à esquerda, um painel por classe com o cabeçalho de
  uma linha (nome, %, meta, a barra com o prumo) e uma `DataTable` dos ativos:
  ativo (botão que abre a folha), quantidade · preço · PM (`wideOnly`), valor,
  rentabilidade com sinal, % da classe × ideal (a barra fina na célula) com o
  selo "↓ abaixo do ideal"/"⚠ acima do teto", nota. À direita: a rosca por
  classe, as roscas por setor (saem de dentro da classe), o histórico de
  aportes recolhido.
- Classes sem ativo numa linha de chips (como hoje "Sem meta e sem ativo").
- **Folha do ativo** `size="wide"`: duas colunas no desktop — à esquerda hoje,
  quantidade, preço manual, teto, IR, checklist ou nota; à direita Lançamentos
  e Fundamentos. Dicas sob os campos viram ⓘ ("Mudar aqui grava um ajuste…",
  o texto do teto, o do IR). O checklist em linhas compactas (pergunta +
  sim/não à direita). A folha do checklist (aberta do plano) segue o mesmo
  desenho.
- **Ajustes › Importar da B3** (também deste grupo): o passo a passo vira
  lista numerada curta com o resto no ⓘ; a prévia usa `DataTable` com
  `StatusPill` na situação ("entra", "já existe", "conflito"); a conferência
  continua em blocos (é formulário).
- **Meta:** Carteira ≤ 1,3 (antes 3,0); celular ≤ 2,5 (antes 4,1). Folha do
  ativo ≤ 1,2 no desktop (antes 1,9–2,4).

### 7.4 Renda, Desempenho, Proventos automáticos e Impostos (grupo 3)

**Renda**

```
┌ Renda ⓘ ─────────────────────────────────────────── [Lançar provento] ┐
│┌ Meta de renda ───────────┐ Recebido 12m │ Média/mês │ A receber │ YoC ⓘ│
││ R$ 97,91/mês ▬▬▬──── 20% │ (StatStrip)                                 │
│└──────────────────────────┘                                             │
│┌ Mês a mês (220 px) ──────────────────────┐┌ Próximos pagamentos ──────┐│
││ ▮ ▮   ▮ ▮   ▮ ▮   ▮ ▨                     ││ 30/11 KNRI11 Rend. R$ 90 ││
│└──────────────────────────────────────────┘│ Eventos da B3            ││
│┌ Quem pagou ───────────┐┌ Ano por mês ─────┴──────────────────────────┐│
││ rosca + tabela (YoC)  ││ tabela compacta                              ││
│└───────────────────────┘└──────────────────────────────────────────────┘│
│ ▸ Estimativa da carteira de hoje (~R$ 410/mês)  ▸ Proventos lançados (N) │
```

- Parágrafo da tela, "Média do que você recebeu…", a conta do yield on cost, o
  aviso da estimativa: ⓘ. O "estimativa" continua escrito onde o número é
  estimado (a regra da tela).
- Meta ≤ 1,2 (antes 2,6); celular ≤ 2,0 (antes 3,4).

**Desempenho**

- Período nos botões do cabeçalho (`actions`); `StatStrip`: 12 meses, desde o
  início (a frase do CDI no detalhe de cada um), patrimônio, aplicado, ganho,
  proventos. Os dois gráficos lado a lado (`Grid cols={2}`), 240–260 px de
  altura; "Mês a mês" embaixo, inteiro. As notas (o ganho é o do preço, hoje é
  parcial, até quando cada índice saiu) viram ⓘ no título de cada gráfico.
- Meta ≤ 1,0 (antes 1,84).

**Ajustes › Proventos automáticos**: os três cartões em `Grid cols={3}`;
"Como funciona" (seis linhas) recolhido ou no ⓘ. Meta ≤ 1,0, celular −40%.

**Ajustes › Impostos**

- O aviso de responsabilidade fica à vista em **uma linha** (o resto no ⓘ).
- `Grid cols="main-aside"`: à esquerda os meses numa `DataTable` (situação com
  `StatusPill`: "A pagar", "Pago"; marcar como pago por botão na linha); à
  direita o ano (chips), o isentômetro e o saldo de antes do Prumo.
- Declaração em `Grid cols={2}`: Bens e Direitos (tabela com "Copiar" na linha)
  e rendimentos (tabelas por código). "Como cada mês foi apurado", "O que o
  Prumo não calculou" e "Nunca entra na conta" ficam recolhidos, com a
  contagem.
- Os "Pontos a confirmar com contador" continuam ao lado do número que mexem,
  mas como ⓘ azul (o padrão da tela), não caixa de três linhas.
- Meta ≤ 2,0 (antes 4,3).

### 7.5 Análise e Ajustes › Análise, Alertas, casca de Ajustes (grupo 4)

**Análise**

```
┌ Análise da carteira ⓘ ─────────────────────────────── Ajustar as regras ┐
│┌ 13 pontos a ver ─┐┌ Diversificação ┐┌ Dividendos ┐┌ Rentab. ┐┌ Qualidade┐│
││ prumo · 26 regras││ 33,3%  ▬▬───    ││ 40%        ││ 66,7%   ││ 66,7%    ││
││ Perfil: Conserv. ││ 3 ok·6 ver·1 s/d││ …          ││ …       ││ …        ││
││ agora ▨▨▨▬ metas ││                 ││            ││         ││          ││
│└──────────────────┘└─────────────────┘└────────────┘└─────────┘└──────────┘│
│┌ Diversificação: pontos a ver ────────────────────────────────────────────┐│
││ Estado      Regra                              Agora   Limite  O que fazer││
││ ⚠ Ponto a ver Nenhum ativo acima de 15%          70,8%   15%    Aportar → ││
││   ▸ Tesouro IPCA+ 2035 70,76% · ITSA4 23,31% (achados recolhidos)         ││
││ ▸ 3 regras ok · 1 sem dado                                                ││
│└───────────────────────────────────────────────────────────────────────────┘│
│ (um painel por dimensão, em Grid cols={2} no ≥ 1000 px)                     │
│┌ Pontos de atenção ─┐┌ Destaques de 12 meses ┐┌ Referências de preço ⓘ ──┐│
││ ITSA4 ⚠ ROE 4,1%…  ││ Subiram / Caíram      ││ Ativo Bazin Graham Teto … ││
│└────────────────────┘└───────────────────────┘└───────────────────────────┘│
│ ▸ De onde vêm os dados                                                      │
```

- Primeira dobra: o veredito com o perfil de risco compacto (as duas barras
  finas, os percentuais escritos, os limiares no ⓘ) e os **quatro cartões de
  dimensão lado a lado** (`Grid cols={4}` ou `auto`): pontuação grande, o
  número exato em contagens ("3 ok · 6 a ver · 1 sem dado") e o `meter`.
- Regras numa `DataTable` por dimensão: `StatusPill`, a frase, Agora, Limite
  (ou o índice), "O que fazer" como link curto. A conta ("0% da carteira está
  no exterior") e o "O que fazer" por extenso vão para o ⓘ da linha; os
  achados de cada regra (os cartões `Finding`) ficam num recolher na própria
  linha, com o nome dos ativos à vista. As regras **ok, sem dado e desligadas**
  ficam num recolher "N regras ok · M sem dado" no fim de cada dimensão.
- Pontos de atenção, destaques e referências em `Grid cols={3}`; Referências
  numa `DataTable` (ativo, Bazin, Graham, pelas duas, seu teto, hoje, e o botão
  "Usar como teto" na linha). Os três parágrafos de fórmula e o aviso
  "referência pública" viram ⓘ no título do painel (o aviso continua escrito,
  em uma linha).
- "De onde vêm os dados" recolhido no fim.
- **Meta:** ≤ 3,0 telas (antes 11,2); celular ≤ 7 (antes 17).

**Ajustes › Análise**: as quatro dimensões em `Grid cols={2}`; cada regra numa
linha compacta (frase com o campo do limite, o interruptor à direita), o porquê
de cada uma no ⓘ. Meta ≤ 2,0 (antes 4,7).

**Ajustes › Alertas**: `Grid cols="main-aside"`: à esquerda Telegram e "O que
avisar" (linhas compactas, explicação no ⓘ); à direita "Quando e com quê" e o
exemplo do resumo. Meta ≤ 1,2 (antes 2,2).

**Casca de Ajustes** (`SettingsScreen`): a partir de 1000 px de conteúdo, a
lista das seções vira uma coluna à esquerda (180–200 px, fixa ao rolar) e a
seção ocupa o resto (`.page` em volta, para as grades das seções enxergarem a
largura certa). No celular, as pílulas numa linha só que rola de lado (hoje
quebram em 2–3 linhas).

## 8. Etapa 2: grupos e arquivos

Quatro agentes em paralelo, um por grupo. Cada um mexe **só** nos arquivos do
seu grupo (e nos testes que os acompanham). Classe CSS de outro grupo pode ser
**usada**, nunca editada; precisou de algo comum, pede a quem integra (ou faz
no próprio arquivo com seletor do próprio bloco).

### Grupo 1 — Aportar, Metas, Critérios, Objetivos

- `features/contribute/**` (`ContributeScreen`, `PlanResult`,
  `ContributionSheet`, testes, `icons.test.tsx`)
- `features/cash/**`, `features/onboarding/**`
- `features/goals/**`, `features/criteria/**`, `features/allocation/**`
- `features/objectives/**` (tela, folha, `PlanObjectives` e testes)
- `features/income/PlanIncomeLine.tsx`, `features/income/IncomeInPlan.test.tsx`
- `lib/planText.ts`, `lib/celebrate.ts`, `lib/goals.ts`, `lib/objectives.ts`
- `styles/contribute.css`, `styles/goals.css`, `styles/criteria.css`,
  `styles/objectives.css`

### Grupo 2 — Carteira, folhas do ativo, Importar da B3

- `features/portfolio/**` (inclui `EditAssetSheet`, `RateAssetSheet` — a folha
  do checklist que o Aportar abre —, `AddAssetSheet`, `AssetForm`,
  `SymbolCombobox`, `ContributionHistory`, `ProfitSummary`, `SectorBreakdown`,
  testes, `icons.test.tsx`)
- `features/ledger/**`, `features/fundamentals/**`, `features/importB3/**`
- `features/analysis/AnalysisSummary.tsx` e `AnalysisSummary.test.tsx` (o
  resumo da Análise que mora na Carteira)
- `lib/ideal.ts`, `lib/sectors.ts`
- `styles/portfolio.css`, `styles/health.css` (paleta e rosca de setor, resumo
  na Carteira), `features/ledger/ledger.css`,
  `features/fundamentals/fundamentals.css`, `features/importB3/importB3.css`

### Grupo 3 — Renda, Desempenho, Proventos automáticos, Impostos

- `features/income/**`, **exceto** `PlanIncomeLine.tsx` e
  `IncomeInPlan.test.tsx`
- `features/incomeAuto/**`, `features/performance/**`, `features/tax/**`
- `styles/income.css`, `features/incomeAuto/incomeAuto.css`,
  `features/performance/performance.css`, `features/tax/tax.css`

### Grupo 4 — Análise, Ajustes › Análise, Alertas, casca de Ajustes

- `features/analysis/**`, **exceto** `AnalysisSummary.tsx` e o teste dele
- `features/settings/**` (a casca de Ajustes e `sections.ts`)
- `features/alerts/**`
- `lib/analysisText.ts`, `lib/healthText.ts`, `lib/alertsText.ts`
- `styles/analysis.css` (inclui `.rule-row`/`.rule-list`, que Objetivos e
  Alertas usam: muda a classe, mantém o contrato), `styles/settings.css`,
  `styles/alerts.css`

### Congelados depois da etapa 1

Ninguém da etapa 2 edita (mudança aqui passa por quem integra, numa rodada à
parte):

- `styles/tokens.css`, `styles/base.css`, `styles/components.css` (inclui os
  padrões compartilhados: `.asset-form`, `.chips-field`, `.danger-zone*`,
  `.income-tag*`, `.income-confirm`, `.rule-row__error`, `.limit-bar*`),
  `styles/layout.css`, `styles/icons.css`
- `components/**` (as primitivas `Layout`, `DataTable`, `Disclosure`,
  `InfoTip`, `Toggletip`, `Sheet`, e todo componente comum)
- `lib/motion.ts` e o resto de `lib/` que não está num grupo acima
- `app/**` (casca, rotas, provedores), `main.tsx`
- `api/**` (o contrato não muda nesta frente)
- `test/handlers.ts`, `test/server.ts`, `test/setup.ts`, `test/render.tsx`,
  `test/fixtures.ts`, `test/icons.ts` e os `test/*Handlers.ts` existentes
  (dado de teste novo vai num arquivo novo do grupo)
- `demo/**`, `scripts/measure-layout.mjs`, `vite.config.ts`, `package.json`,
  `eslint.config.js`
- Este arquivo: cada grupo escreve a medida "depois" e o desvio do plano no
  relatório; quem integra atualiza o § 1. Em `docs/design.md`, cada grupo
  edita só a seção das suas telas.

### O que cada grupo entrega

1. As telas do grupo com `<Page>`/`<PageHeader>` e a grade; listas longas em
   `DataTable`; explicação no ⓘ conforme § 5; só tokens de espaço e de
   movimento.
2. Testes de tela atualizados (o texto que virou ⓘ é conferido abrindo o ⓘ),
   olho fechado conferido no que mudou de lugar, `npm run build` e `npm test`
   2× verdes.
3. A medida "depois" (§ 9) nas cinco larguras, com as capturas fora do
   repositório, contra a meta da seção da tela.

## 9. Como medir

```bash
cd frontend
npm run dev:demo            # http://localhost:5180, carteira fictícia, sem backend
```

`dev:demo` sobe o Vite no modo `demo`: o `main.tsx` liga o service worker do
msw (`src/demo/startDemo.ts`) com os handlers dos testes (`test/handlers.ts`)
mais os da demonstração (`src/demo/demoHandlers.ts`: Renda, Desempenho,
Alertas, objetivos e um plano com renda, referências e aviso de teto). O
worker vem direto do pacote `msw` instalado (plugin em `vite.config.ts`) e o
ramo inteiro sai do build de produção. Recarregar volta ao começo. Nenhum dado
é real.

As medidas e capturas saem do script, com o Playwright instalado **fora** do
repositório (não é dependência do projeto):

```bash
mkdir -p /tmp/pw && cd /tmp/pw && npm init -y && npm i playwright@1.65.1 && npx playwright install chromium
node <repo>/frontend/scripts/measure-layout.mjs <pasta-das-capturas>
# só algumas larguras:          MEASURE_VIEWPORTS=1440x900,1920x1080
# sem animação (comparar pixel): MEASURE_REDUCED_MOTION=1
# outro endereço:                MEASURE_URL=http://localhost:5180
```

Ele abre cada tela e folha em 1440×900, 1920×1080, iPad deitado e em pé e
celular, espera a rede e as animações, mede a altura, a largura útil e quais
títulos ficam acima da dobra, tira uma captura de página inteira e escreve
`medidas.json` e `medidas.md`. As capturas "antes" estão em
`D:\REPO\prumo-wt\layout-shots\antes\` (fora do repositório).
