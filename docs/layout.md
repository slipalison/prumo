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

### Depois da etapa 2 (consolidado)

Medido de novo, no modo de demonstração, depois da integração dos quatro
grupos e dos acertos da frente `layout-fix` (§ 10), com a mesma carteira
fictícia e o mesmo script (§ 9). Cada célula é **antes → depois**, em telas
(altura da página ÷ altura da janela; 1,02 é o piso da casca, que nunca é menor
que a janela).

| Tela | 1440×900 | 1920×1080 | iPad 1366×1024 | iPad 1024×1366 | Celular 390×844 |
|---|---|---|---|---|---|
| Aportar (vazio) | 1,03 → 1,02 | 1,02 → 1,01 | 1,02 → 1,02 | 1,02 → 1,01 | 1 → 1 |
| Aportar (plano calculado) | 2,2 → 1,02 | 1,83 → 1,01 | 2,11 → 1,02 | 1,71 → 1,02 | 4,14 → 2,48 |
| Carteira | 3,01 → 1,19 | 2,51 → 1,01 | 2,64 → 1,1 | 2,02 → 1,13 | 4,05 → 2,47 |
| Renda | 2,61 → 1,23 | 2,17 → 1,02 | 2,28 → 1,08 | 1,61 → 1,15 | 3,42 → 2,26 |
| Desempenho | 1,84 → 1,02 | 1,53 → 1,01 | 1,62 → 1,02 | 1,27 → 1,01 | 2,74 → 1,86 |
| Análise | 11,16 → 2,76 | 9,3 → 2,25 | 9,9 → 2,5 | 7,85 → 2,27 | 17,25 → 6,4 |
| Ajustes › Metas | 1,44 → 1,02 | 1,2 → 1,01 | 1,29 → 1,02 | 1,02 → 1,01 | 2,77 → 1,81 |
| Ajustes › Critérios | 1,03 → 1,02 | 1,02 → 1,01 | 1,02 → 1,02 | 1,02 → 1,01 | 2,13 → 1,63 |
| Ajustes › Objetivos | 1,03 → 1,02 | 1,02 → 1,01 | 1,02 → 1,02 | 1,02 → 1,01 | 1,36 → 1 |
| Ajustes › Análise | 4,7 → 1,61 | 3,92 → 1,24 | 4,16 → 1,47 | 3,69 → 1,34 | 8,78 → 4,49 |
| Ajustes › Importar da B3 | 1,03 → 1,02 | 1,02 → 1,01 | 1,02 → 1,02 | 1,02 → 1,01 | 1,71 → 1,05 |
| Ajustes › Impostos | 4,3 → 1,93 | 3,58 → 1,41 | 3,82 → 1,71 | 3,1 → 1,77 | 6,38 → 4,41 |
| Ajustes › Alertas | 2,18 → 1,02 | 1,82 → 1,01 | 1,99 → 1,02 | 1,6 → 1,01 | 3,65 → 2,25 |
| Ajustes › Proventos automáticos | 1,03 → 1,02 | 1,02 → 1,01 | 1,02 → 1,02 | 1,02 → 1,01 | 1,91 → 1 |

Folhas (conteúdo ÷ altura da folha; 1 cabe sem rolar por dentro):

| Folha | 1440×900 | 1920×1080 | iPad 1366×1024 | iPad 1024×1366 | Celular 390×844 |
|---|---|---|---|---|---|
| Folha do ativo (ação) | 1,89 → 1 | 1,82 → 1 | 1,82 → 1 | 1,82 → 1 | 2,31 → 1,71 |
| Folha do ativo (FII) | 2,4 → 1,17 | 2,31 → 1,13 | 2,31 → 1,13 | 2,31 → 1,14 | 2,89 → 2,04 |
| Folha novo ativo | 1 → 1 | 1 → 1 | 1 → 1 | 1 → 1 | 1 → 1 |
| Folha do checklist | 2,45 → 1,17 | 2,36 → 1,13 | 2,36 → 1,13 | 2,36 → 1,14 | 2,98 → 2,13 |
| Folha registrar aporte | 1 → 1 | 1 → 1 | 1 → 1 | 1 → 1 | 1 → 1 |
| Folha lançar provento | 1 → 1 | 1 → 1 | 1 → 1 | 1 → 1 | 1,18 → 1,18 |
| Folha meta de renda | 1 → 1 | 1 → 1 | 1 → 1 | 1 → 1 | 1 → 1 |
| Folha objetivo | 1 → 1 | 1 → 1 | 1 → 1 | 1 → 1 | 1 → 1 |
| Folha teto | 1 → 1 | — → 1 | — → 1 | 1 → 1 | 1 → 1 |

Rolagem lateral, medida em todas as telas e folhas, nas cinco larguras e nos
dois temas (`document.documentElement.scrollWidth > clientWidth`, e o mesmo
dentro de qualquer bloco que rola): **nenhuma página rola de lado**, e no desktop
e no iPad (1366 e 1024) nenhum bloco rola de lado por dentro. Sobram só os blocos
que rolam **de propósito** no celular: a lista de seções de Ajustes (uma linha
de pílulas) e as tabelas de 13 colunas de Renda (Ano por mês) e Desempenho (Mês
a mês). Em 768–820 px (iPad mini e Air em pé, fora da lista medida) as tabelas
de Análise, Renda, Desempenho e Impostos também rolam por dentro, nunca a página.

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
  ficam só com o ícone (o nome acessível não muda). O trilho e o item quase não
  têm recheio (74 px por item): "Desempenho", o nome mais largo, cabe inteiro, sem
  reticências.
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
- A barra de abas tem sete telas desde a Agenda: vai até 560 px de largura e,
  abaixo de 600 px de janela, só a aba aberta mostra o nome (as outras ficam com
  o ícone, e o nome continua no link para o leitor de tela). "Desempenho", o
  nome mais largo, cabe inteiro em 320 px.
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

**Revisão do Aportar (frente aporte-comprei).** A pessoa pediu outra ordem e um
"Comprei" por ativo; este desenho **substitui** o da caixa acima.

```
┌ Aportar ────────────────────────────────────────────────────────────────┐
│┌ Quanto você vai aportar? ┐ ┌ A carteira depois do aporte  [Antes|Depois]┐│
││ R$ 6.000,00              │ │ rosca   Ações Nac.  ▬▬▬│▬   Fundos Imob. ▬││
││ + R$ 812 em caixa Incluir│ │         Ações Int.  ▬▬▬│▬   Cripto      ▬││
││ [500][1.000][2.000][5mil]│ │         Renda Fixa  ▬▬▬│▬                 ││
││ [Calcular]               │ └─────────────────────────────────────────────┘│
│└──────────────────────────┘                                                │
│┌ Compre 4 ativos ──────────────────────────────── [Registrar 4 comprados] ┐│
││ com R$ 5.947,92 · troco de R$ 52,08        4 de 4 marcados · [atalho]    ││
││┌ITSA4────┐┌BTLG11───┐┌TRXF11───┐┌BTC──────┐   (quebra quando não cabe)    ││
│││R$ 56,00 ││R$ 756,40││R$ 914,77││R$1.555  │                               ││
│││✓ Comprado││✓ Comprado││...                                              ││
│││Plano: 243││[Editar][Desfazer]                                            ││
││└─────────┘└─────────┘└─────────┘└─────────┘                               ││
│└───────────────────────────────────────────────────────────────────────────┘│
│┌ Avisos (2) ──────────────────┐ ┌ Seus objetivos ────────────────────────┐ │
```

- **Linha de cima:** `Grid cols="aside-main"` com o campo (1/3) e a carteira
  (2/3), de altura igual. A carteira é um painel só, `BalancePanel`, que serve
  aos dois estados.
- **Antes do cálculo:** o painel mostra a carteira **de hoje** (barras com a
  meta, rosca, sem Antes/Depois). Decisão: em vez de um espaço vazio ou de
  centralizar o campo, mostrar o ponto de partida mantém o campo no mesmo lugar e
  tamanho quando o plano chega (nada pula), custa zero clique e responde ao que
  a pessoa se pergunta enquanto decide o valor. Sem nenhuma classe com meta ou
  valor, o campo volta a ficar centrado e sozinho.
- **Com o plano:** o mesmo painel vira Antes/Depois (nasce no "antes" e
  escorrega para o "depois" em 400 ms). Painel ≥ 620 px: as barras em duas colunas.
- **Embaixo, na largura inteira:** "Compre N ativos", os cartões em
  `Grid cols="auto" minWidth="200px"` com `auto-fill` (uma linha até onde couber;
  poucos ativos não esticam), depois Avisos e Objetivos lado a lado.
- **Uma coluna só** (< 1000 px de conteúdo): o DOM segue a ordem do desktop
  (campo, carteira, plano, avisos) e vale também aqui, sem `order` no CSS: a
  carteira (hoje, ou depois do aporte) fica logo abaixo do campo, depois "Compre
  N ativos", depois Avisos e Objetivos. Leitura, tab e tela andam juntas.
- **"Comprei" por ativo:** botão em cada cartão → folha pequena (`BoughtSheet`,
  quantidade + preço) → cartão marcado ("✓ Comprado: …", "Plano: …" se
  diferente) com Editar/Desfazer. "Registrar N comprados" no canto do painel (flutua
  no rodapé abaixo de 960 px) → conferência (`ContributionSheet`) → um
  `POST /contributions` só com os marcados. Detalhes em docs/design.md, Aportar.
- **Folha:** `Sheet` guarda quem tinha o foco ao abrir num efeito de layout, antes
  dos efeitos do conteúdo; um campo com `focusOnMount` dentro da folha rodava antes
  e se passava por "quem abriu", e o foco não voltava ao botão ao fechar.
- **Defeito do "R$ 0,00" dourado:** o campo do valor ganha foco na abertura e
  `MoneyInput` selecionava o texto inteiro, para o próximo dígito substituir o
  valor; o `::selection` do app é o dourado cheio (`base.css`), então o zero
  aparecia como um bloco de seleção. Correção: com o valor zerado não há o que
  substituir, o cursor vai para o fim (o dígito entra como centavo, igual) e
  nada é selecionado; com valor, a seleção segue (o próximo dígito começa um
  valor novo), mas no campo de dinheiro ela é um véu dourado de 28% com o texto
  na cor própria (`.money-field__input::selection`), nos dois temas.

**Medido (demonstração, plano de R$ 6.000 com o caixa incluído, 4 ativos):**

| Aportar | 1440×900 | 1920×1080 | iPad 1366×1024 | iPad 1024×1366 | Celular 390×844 |
|---|---|---|---|---|---|
| Vazio (carteira de hoje ao lado) | 1,02 | 1,01 | 1,02 | 1,01 | 1,03 |
| Com plano | 1,23 | 1,01 | 1,10 | 1,01 | 2,64 |
| Com os 4 marcados | 1,25 | 1,02 | 1,12 | 1,01 | 2,62 |

O desktop passou de 1,02 (desenho do grupo 1) para 1,23 porque os cartões agora
têm o botão e ocupam a largura inteira, embaixo da carteira, como a pessoa
pediu; a meta pedida era ~1,2. No celular o botão de cada cartão custa ~0,15 tela
(2,48 → 2,64). Nenhuma rolagem lateral em nenhuma largura.

### 7.2 Ajustes › Metas, Critérios e Objetivos (grupo 1)

- **Metas:** perfis prontos numa linha de botões segmentados (a descrição de
  cada perfil no ⓘ); os sete controles em `Grid cols={2}` dentro de um painel;
  à direita (`main-aside`) o anel de soma, a rosca, Equilibrar/Salvar e o
  **Limite por ativo** (o texto longo do limite no ⓘ). Meta ≤ 1,0 (antes 1,44).
- **Critérios:** o parágrafo de abertura vira ⓘ; as duas listas como hoje
  (já lado a lado). Meta ≤ 1,0 em 1440 e −30% no celular.
- **Objetivos:** cartões em `Grid cols="auto" minWidth="340px"` (a curva
  projetada precisa da largura); as premissas ficam à vista (é a regra da tela:
  nenhuma data sem a conta), numa lista de definição compacta, rótulo e número
  numa linha, a origem no ⓘ. Medido depois da projeção (onda 3): 1,2 tela em
  1440 com três objetivos ativos e um concluído; sem rolagem lateral em 1440 e
  390, claro e escuro.

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

**Depois (grupo 2, feito).** Medido pelo grupo no modo de demonstração, em
telas (altura da página ÷ altura da janela; 1,01 é o piso da casca):

| Tela | 1440×900 | 1920×1080 | iPad 1366×1024 | iPad 1024×1366 | Celular 390×844 |
|---|---|---|---|---|---|
| Carteira (antes 3,01 / 2,51 / 2,64 / 2,02 / 4,05) | 1,19 | 1,01 | 1,10 | 1,13 | 2,47 |
| Ajustes › Importar da B3, antes de ler (antes 1,03 / 1,02 / 1,02 / 1,02 / 1,71) | 1,02 | 1,01 | 1,02 | 1,01 | 1,25 |

Folhas (conteúdo ÷ altura da folha; antes entre parênteses):

| Folha | Desktop e iPad deitado (1366–1920) | iPad 1024×1366 | Celular |
|---|---|---|---|
| Ativo, ação (1,82–1,89) | 1,00 | 1,00 | 1,71 (antes 2,31) |
| Ativo, FII (2,31–2,40) | 1,13–1,17 | 1,14 | 2,04 (antes 2,89) |
| Checklist aberta do plano (2,36–2,45) | 1,13–1,17 | 1,14 | 2,13 (antes 2,98) |
| Novo ativo | 1,00 | 1,00 | 1,00 |

O que mudou:

- **Carteira:** a faixa de números (patrimônio com o selo ao vivo, lucro total
  com as três parcelas no ⓘ, "abaixo do ideal" e o resumo da Análise) em três ou
  duas colunas conforme a largura; um painel por classe com a barra e o prumo no
  cabeçalho e uma `DataTable` dos ativos; à direita a rosca por classe, os
  setores e o histórico de aportes recolhido. Quando a grade vira uma coluna, os
  painéis laterais ficam lado a lado e o setor vira recolher só no celular.
- **Folha do ativo** em duas colunas (`size="wide"`), dicas em ⓘ e o checklist em
  linhas compactas; a folha do checklist segue o mesmo desenho. Ação: 1,0 (cabe
  sem rolar por dentro); FII e checklist: 1,13–1,17 (meta 1,2).
- **Importar da B3:** passo a passo curto com os limites num recolher, os
  números da prévia em faixas e a tabela por ativo em `DataTable` com
  `StatusPill` na situação.
- Em `components/`: nada foi alterado pelo grupo.

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
- **Onda 3 (frente desempenho):** "Patrimônio × aportado" no lugar de
  "Patrimônio no tempo" (a ponte do período em `StatStrip inset`), e uma
  segunda linha `Grid cols="aside-main"` com **Por classe** e **Ano a ano** (o
  antigo "Mês a mês"). Medido no modo de demonstração (12 meses; Tudo +
  Acumulado + IFIX; 5 anos + IPCA), temas claro e escuro: 1920×1080 1,16
  tela, 1440×900 1,35–1,47, iPad 1024×1366 1,24, celular 390×844 3,33–3,35
  (antes 1,01 / 1,02 / 1,01 / 1,86: são dois painéis novos). Os gráficos foram
  conferidos em 1920 com a revelação por recorte (`a38adde`): linha inteira e
  tracejado visível. Nenhuma rolagem lateral de página; no desktop
  e no iPad nenhum bloco rola de lado (o acumulado tem uma casa decimal para a
  tabela caber em 1024); no celular só a tabela do ano a ano rola por dentro,
  como a de antes. Nenhum texto passa da borda do painel.

**Ajustes › Proventos automáticos**: os três cartões em `Grid cols={3}`;
"Como funciona" (seis linhas) recolhido ou no ⓘ. Meta ≤ 1,0, celular −40%.

**Ajustes › Impostos**

- O aviso de responsabilidade fica à vista em **uma linha** (o resto no ⓘ).
- `Grid cols="main-aside"`: à esquerda os meses numa `DataTable` (situação com
  `StatusPill`: "A pagar", "Em atraso", "Pago"; marcar como pago e "Multa e
  juros" por botão na linha) e, embaixo, o prejuízo a compensar mês a mês; à
  direita o ano (chips), o isentômetro e o saldo de antes do Prumo.
- Declaração em `Grid cols={2}`: Bens e Direitos (tabela com "Copiar" na linha)
  e rendimentos (tabelas por código), e os rendimentos por fonte pagadora em
  largura inteira. "Como cada mês foi apurado", "O que o Prumo não calculou" e
  "O que o Prumo cobre" ficam recolhidos, com a contagem.
- Os "Pontos a confirmar com contador" continuam ao lado do número que mexem,
  mas como ⓘ azul (o padrão da tela), não caixa de três linhas.
- Meta ≤ 2,0 (antes 4,3).

**Depois (grupo 3, feito).** Medido no modo de demonstração, em telas (altura
da página ÷ altura da janela; 1,02 é o piso da casca, que nunca é menor que a
janela):

| Tela | 1440×900 | 1920×1080 | iPad 1366×1024 | iPad 1024×1366 | Celular 390×844 |
|---|---|---|---|---|---|
| Renda (antes 2,61 / 2,17 / 2,28 / 1,61 / 3,42) | 1,23 | 1,02 | 1,08 | 1,16 | 2,27 |
| Desempenho (antes 1,84 / 1,53 / 1,62 / 1,27 / 2,74) | 1,02 | 1,01 | 1,02 | 1,01 | 1,86 |
| Ajustes › Impostos (antes 4,30 / 3,58 / 3,82 / 3,10 / 6,38) | 1,81 | 1,40 | 1,63 | 1,79 | 4,66 |
| Ajustes › Proventos automáticos (antes 1,03 / 1,02 / 1,02 / 1,02 / 1,91) | 1,02 | 1,01 | 1,02 | 1,01 | 1,11 |

As folhas de lançar provento e de meta de renda não mudaram (1,0; 1,18 no
celular na de provento).

O que mudou, e onde o desenho do plano foi outro:

- **Renda:** a meta e os quatro números (o yield on cost é um deles, com a
  conta no ⓘ) lado a lado no topo; o mês a mês e o "Quem pagou" na coluna de 2/3
  e, na de 1/3, próximos pagamentos, eventos da B3 e estimativa; Ano por mês em
  largura inteira e Proventos lançados recolhido. **Desvio:** a estimativa
  fica à vista (e não recolhida) porque o número dela é o que a tela diz; e as
  barras se desenham na largura real do painel, com a barra limitada a 48 px.
  No celular a rosca de "Quem pagou" sai (a lista já diz o nome, o valor e a
  parte): 2,27 telas, acima da meta de 2,0.
- **Desempenho:** período nos botões do cabeçalho, seis números numa faixa,
  patrimônio e rentabilidade lado a lado (196 px de gráfico, 270 px a partir de
  1500 px de conteúdo), Mês a mês embaixo.
- **Impostos:** 1,81 em 1440×900 (meta 2,0). **Desvio:** o isentômetro, o saldo
  de antes e os recolheres "O que o Prumo não calculou" e "O que o Prumo cobre"
  (antes "Nunca entra na conta") ficam na coluna da direita, ao lado dos meses, em vez de no fim da
  tela; "Como cada mês foi apurado" fica recolhido embaixo da declaração.
  No celular o mês sem movimento mostra só o nome: 4,66 telas.
  **Onda 3 (cobertura de IR):** a tabela dos meses ganhou a coluna do prejuízo
  a compensar (o cabeçalho quebra em duas linhas e, abaixo de 670 px, a linha
  do DARF quebra entre o código e o valor; empilhada, a situação toma a linha
  inteira), e a declaração junta os rendimentos numa tabela por fonte pagadora.
  Medido no modo de demonstração, nos dois temas, sem rolagem lateral nem texto
  cortado: 2,18 telas em 1440×900, 1,96 no iPad deitado, 1,92 em pé e 5,16 no
  celular (acima da meta de 2,0 no desktop: a tabela tem um dado a mais por
  mês, e a calculadora do atraso e o IR retido ficam em folha e no recolher).
- **Proventos automáticos:** dois painéis lado a lado (a chave e os eventos em
  ações; desligado, só a chave, em largura inteira) e "Como funciona" recolhido,
  em vez de três painéis em `Grid cols={3}`.
- Em `components/`: nada foi alterado. Os arquivos novos são de `features/`
  (`income/FoldPanel` e os painéis de `tax/`).

**Renda, explorar (rodada depois da integração).** A pessoa pediu para
*explorar* "Quem pagou" e a estimativa, e achou um vazio enorme no meio da tela
(a rosca e a legenda ocupavam só o topo de um painel esticado até o fim da coluna
da direita). O desenho passou a ser:

```
┌ Quanto sua carteira paga? ⓘ ─────────────────────────── [Lançar provento] ┐
│ Meta de renda        │ Recebido 12m │ Média/mês │ A receber │ YoC ⓘ        │
│┌ Mês a mês ────────────────────────┐┌ Próximos pagamentos ┐ (mesma altura,│
││ barras                            ││ lista que rola      │  rola por     │
│└───────────────────────────────────┘└─────────────────────┘  dentro)      │
│┌ Quem pagou ───────────── [Filtro ✕]┐┌ Estimativa da carteira de hoje ────┐│
││ rosca   Pagador  Recebido % YoC    ││ ~R$ 2.052/mês   estimativa         ││
││ (hover) ▸ ALFA3 … ▸ Outros (19) →  ││ 5 maiores · Ver todos os 28 ativos ││
│└────────────────────────────────────┘└─────────────────────────────────────┘│
│ Ano por mês                                                                │
│ ▸ Proventos lançados (20)                                                  │
```

- **Altura constante entre os estados.** A tabela de "Quem pagou" tem altura
  fixa (20 rem) e rola por dentro: "Outros (19)" não empurra a página, não estica
  o painel ao lado e não deixa buraco nele. Os "Próximos pagamentos" tomam a
  altura do mês a mês (posição absoluta dentro do painel, que não conta na altura
  da linha) e rolam por dentro; abaixo de 1000 px de conteúdo (uma coluna) a lista
  tem no máximo 20 rem.
- **Quem pagou ao lado da estimativa**, na mesma linha (`Grid cols="main-aside"`,
  os dois painéis esticam à mesma altura). A estimativa mostra os cinco que mais
  rendem e abre a lista inteira numa folha (uma lista longa no painel o alongaria
  e abriria vazio no vizinho).
- **Entrar em Outros, filtrar por pagador e a rosca ao ponteiro**: ver
  `docs/design.md`, Renda, item 4. A rosca própria (`PayerDonut`) é da Renda; a
  `Donut` comum não mudou (`components/` congelado).
- **No celular** a rosca continua fora, e cada pagador é um bloco de duas linhas.
  O filtro por pagador age em Próximos pagamentos e em Proventos lançados.

Medido no modo de demonstração com 25 pagadores ("Outros (19)"), 14 pagamentos a
receber e 28 ativos na estimativa (`demo/demoIncome.ts`; o dado de antes só tinha
3 pagadores e 1 pagamento, por isso o 1,23 de cima). Telas (altura da página ÷
altura da janela), **antes → depois** com o mesmo dado:

| Renda | 1440×900 | 1920×1080 | iPad 1366×1024 | iPad 1024×1366 | Celular 390×844 |
|---|---|---|---|---|---|
| Quem pagou fechado | 1,83 → 1,48 | 1,53 → 1,23 | 1,61 → 1,30 | 1,80 → 1,62 | 3,45 → 3,11 |
| Dentro de "Outros (19)" | — → 1,48 | — → 1,23 | — → 1,30 | — → 1,63 | — → 3,17 |

Sem rolagem lateral da página em nenhuma largura. Em 1440×900 a linha de baixo
fecha com os dois painéis em ~425 px, sem vazio. Resta uma folga de ~35 px acima
e abaixo da rosca e da tabela quando o painel é mais alto que ela (o conteúdo fica
centrado).

**Renda, estimativa líquida (frente estimativa-liquida).** A estimativa passou
a mostrar o líquido do IR retido na fonte em destaque e o bruto ao lado, a fonte de cada
ativo, a média de até 5 anos e a marca "acima do habitual" (regra em
`docs/proventos-automaticos.md`, seção 9; desenho em `docs/design.md`, Renda,
item 5). O painel ficou assim:

```
┌ Estimativa da carteira de hoje ⓘ ─────────── estimativa ┐
│ ~R$ 2.052,40 por mês, líquido                            │
│ bruto ~R$ 2.133,80 por mês                               │
│ ⚠ ~R$ 331,20/mês de 3 ativos acima do habitual ⓘ         │
│ Ativo              Por mês          Média 5 anos         │
│ AL ALFA3 [B3]      ~R$ 130,00       ~R$ 123,50           │
│                    bruto R$ 144,44                       │
│ BR BRAV11 [B3]     ~R$ 125,80       ~R$ 69,89            │
│                                     (⚠ acima)            │
│ … (quatro linhas)                                        │
│ Ver todos os 28 ativos →                                 │
└──────────────────────────────────────────────────────────┘
```

- **Quatro linhas, não cinco.** Com o bruto embaixo do líquido e a linha de
  "acima do habitual", cinco linhas levavam o painel a 552 px em 1440×900, mais
  alto que o "Quem pagou" ao lado (que ganhava ~65 px vazios em cima e embaixo
  da rosca: o vazio que a rodada anterior tinha tirado). Com quatro, e a
  segunda linha de cada célula dentro da altura de linha de sempre (40 px,
  `padding-block` de 4 px só nas tabelas da estimativa), o painel fica em
  440 px.
- **A marca na coluna da média**, e não embaixo do código: assim a coluna do
  ativo tem uma linha só. No painel o texto visível é "acima" (o leitor de
  tela ouve "acima do habitual"; a linha do total diz por extenso); na folha,
  por extenso.
- **No celular** o ícone do ativo sai da tabela do painel (`@container` até
  380 px): sem isso as três colunas rolavam de lado dentro do painel.
- A folha "Estimativa por ativo" ganhou Por cota com o bruto embaixo e a Média
  5 anos; as colunas ficaram mais juntas (`padding-inline` de 8 px) para as
  cinco caberem sem rolar de lado.

Medido no modo de demonstração (`demo/demoIncome.ts`: 28 ativos, 3 acima do
habitual, 2 pelo Yahoo), com Playwright headless, altura da página ÷ altura da
janela, **antes → depois**:

| Renda | 1440×900 | 1920×1080 | iPad 1366×1024 | iPad 1024×1366 | Celular 390×844 |
|---|---|---|---|---|---|
| Página | 1,48 → 1,49 | 1,23 → 1,25 | 1,30 → 1,31 | 1,62 → 1,63 | 3,11 → 3,16 |
| Painel da estimativa (px) | ~425 → 440 | ~425 → 440 | → 440 | → 440 | → 463 |

Sem rolagem lateral da página, da tabela do painel nem da folha em nenhuma
largura, com os valores à mostra ou ocultos; com o olho fechado nenhum "R$"
seguido de número aparece na página nem na folha.

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

**Depois (grupo 4, feito).** Medido no modo de demonstração, em telas (altura
da página ÷ altura da janela; 1,02 é o piso da casca):

| Tela | 1440×900 | 1920×1080 | iPad 1366×1024 | iPad 1024×1366 | Celular 390×844 |
|---|---|---|---|---|---|
| Análise (antes 11,16 / 9,30 / 9,90 / 7,85 / 17,25) | 2,76 | 2,25 | 2,50 | 2,27 | 6,40 |
| Ajustes › Análise (antes 4,70 / 3,92 / 4,16 / 3,69 / 8,78) | 1,61 | 1,24 | 1,47 | 1,30 | 4,49 |
| Ajustes › Alertas (antes 2,18 / 1,82 / 1,99 / 1,60 / 3,65) | 1,02 | 1,01 | 1,02 | 1,01 | 2,25 |

A folha de teto (aberta da tabela de referências) segue em 1,0 (ver § 7.6, onde
ela virou a escolha do teto).

O que mudou, e onde o desenho foi outro:

- **Análise, primeira dobra:** o veredito com o perfil de risco compacto (os
  limiares no ⓘ) à esquerda e as quatro pontuações em **2 × 2** ao lado (no
  plano eram cinco cartões numa linha: com o perfil dentro do veredito, os
  cartões de pontuação ficavam altos e vazios). Abaixo de 1100 px de conteúdo,
  o veredito ocupa a linha e as pontuações ficam 4 lado a lado (2 × 2 no
  celular). Cada pontuação tem o selo "N a ver" e leva até a tabela da dimensão.
- **Regras:** uma `DataTable` por dimensão em **largura inteira** (o plano dizia
  `Grid cols={2}`): Regra, Estado (`StatusPill`), **Diagnóstico** (a conta,
  em uma linha, à vista — o pedido era "diagnóstico em uma linha"), Agora (com a
  barra fina do limite), Limite ("máx. 15%", "mín. 10%", "CDI 13,20%") e O que
  fazer (o link à vista, o texto inteiro com o valor do aporte no ⓘ). Em duas
  colunas o diagnóstico não caberia. Os achados das sete regras originais são
  linhas do diagnóstico com o ícone dos ativos (o cartão `Finding` saiu); o
  "o que fazer" de cada achado vai junto no ⓘ. Ok, sem dado e desligadas num
  recolher no fim de cada dimensão. Entre 561 e 1000 px de tabela (iPad em pé)
  cada regra vira duas linhas; abaixo de 560 px, um bloco.
- **Embaixo:** Pontos de atenção e Destaques lado a lado; Referências de preço
  numa tabela em largura inteira (o plano punha as três em `Grid cols={3}`: a
  tabela de sete colunas não cabe em 1/3). "De onde vêm os dados" recolhido,
  com a falha de fonte já na frase fechada.
- **Ajustes › Análise:** duas pilhas de painéis (Diversificação, Dividendos e
  o Bazin; Rentabilidade e Qualidade), de altura parecida, em vez de uma grade
  de 2 colunas linha a linha; a frase da regra é texto corrido com o campo no
  meio e o ⓘ colado; com mudança por salvar, a barra de salvar acompanha a
  rolagem no desktop.
- **Alertas:** duas colunas iguais (`Grid cols={2}`, não `main-aside`): com o
  exemplo do resumo na coluna estreita a tela passava de 1,1.
- **Casca de Ajustes:** `<Page>` em volta; coluna de 188 px fixa ao rolar a
  partir de 1000 px de conteúdo; pílulas numa linha que rola no celular, com a
  seção aberta trazida à vista. Seção sem `<Page>` fica limitada a
  `--content-legacy`.
- Em `components/` e nos congelados: nada foi alterado. A `DataTable` comum
  rolava de lado (`overflow-x: auto`) e isso cortava o ⓘ da primeira linha; o G4
  contornou com `overflow: visible` no `analysis.css`. O contorno foi desfeito na
  integração (§ 10): a causa estava na primitiva.

### 7.6 Teto por escolha e referências de FII (frente teto-fii)

**Análise › Referências de preço** virou duas `DataTable` no mesmo painel, com a
legenda à vista: **Ações** (Ativo, Bazin, Graham, Pelas duas, Seu teto, Hoje,
Teto ▾) e **FIIs** (Ativo, Tipo, VP, Rendimento, Pelas duas, Seu teto, Hoje,
Teto ▾). O botão longo "Usar R$ X como meu teto" deu lugar ao "Teto ▾" compacto
(36 px; 44 px com toque), que abre uma folha pequena com a escolha. No celular
cada ativo é um cartão empilhado, e o "Teto ▾" fecha a última linha à direita
(antes ocupava uma linha só para ele).

**A escolha do teto** (`features/ceiling/CeilingPicker`) é uma lista de rádio de
44 px por opção: o nome à esquerda, o valor de hoje e o chip aprova/não aprova à
direita, a conta no ⓘ. A opção marcada ganha fundo afundado e filete dourado;
foco visível no rádio. Na **folha do ativo**, ela fica recolhida numa linha ao
lado de "Usar preço manual" (o lugar da antiga chave "Teto de preço"): "Teto de
preço ⓘ … [Graham · R$ 9,50 ▾]", e abre embaixo no toque — fechada, a folha
tem a mesma altura de antes.

**Ajustes › Análise**: o painel Referências de preço ganhou os dois prêmios
(linhas iguais à do rendimento-alvo) e uma linha com as taxas de hoje.

Medido no modo de demonstração, em telas (altura ÷ altura da janela; folhas:
conteúdo ÷ altura da folha); escuro e claro dão os mesmos números, e o celular
foi medido com o "Teto ▾" já na última linha do cartão. A demonstração
ganhou dois FIIs fictícios (um híbrido e um fundo de fundos) para a Análise
mostrar um FII de cada tipo: a tabela de FIIs tem 6 linhas.

| Tela | 1440×900 | 1920×1080 | iPad 1366×1024 | iPad 1024×1366 | Celular 390×844 |
|---|---|---|---|---|---|
| Análise (antes 2,76 / 2,25 / 2,50 / 2,27 / 6,40) | 3,17 | 2,59 | 2,86 | 2,54 | 7,28 |
| Ajustes › Análise (antes 1,61 / 1,24 / 1,47 / 1,30 / 4,49) | 1,69 | 1,33 | 1,58 | 1,45 | 4,79 |
| Folha do ativo, ação (antes 1,00 / 1,00 / 1,00 / 1,00 / 1,71) | 1,00 | 1,00 | 1,00 | 1,00 | 1,71 |
| Folha do ativo, FII (antes 1,17 / 1,13 / 1,13 / 1,14 / 2,04) | 1,17 | 1,13 | 1,13 | 1,15 | 2,04 |
| Folha do teto, ação (ITSA4) | 1,00 | 1,00 | 1,00 | 1,00 | 1,00 |
| Folha do teto, FII (KNRI11) | 1,00 | 1,00 | 1,00 | 1,00 | 1,00 |

- A Análise cresce pelo que entrou: a segunda tabela, com uma linha por FII (a
  carteira da demonstração tem uma ação e seis FIIs nas referências; antes, só os
  ativos com Bazin ou Graham apareciam). Fica acima da meta de 3,0 em 1440×900
  por 0,17 tela (~150 px, umas três linhas de FII); a altura acompanha quantos
  FIIs a carteira tem.
- A folha do ativo não cresceu: fechada, a escolha do teto ocupa a mesma linha
  da chave antiga; aberta, soma ~230 px.
- Rolagem lateral: nenhuma nova (as que aparecem no celular são as pílulas de
  Ajustes e as tabelas anuais da Renda, de antes).

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
# tema claro (padrão: escuro):  MEASURE_THEME=light
```

Ele abre cada tela e folha em 1440×900, 1920×1080, iPad deitado e em pé e
celular, espera a rede e as animações, mede a altura, a largura útil e quais
títulos ficam acima da dobra, tira uma captura de página inteira e escreve
`medidas.json` e `medidas.md`. As capturas "antes" estão em
`D:\REPO\prumo-wt\layout-shots\antes\` (fora do repositório).

## 10. Integração (frente layout-fix)

Acertos feitos depois dos quatro grupos, com a causa de cada um:

- **ⓘ cortado pela tabela.** `overflow-x: auto` obriga `overflow-y: auto`, e o
  balão (`position: absolute`) era cortado na primeira linha de qualquer
  `DataTable`. Conserto na primitiva, em dois passos: (1) o `Toggletip` abre na
  **camada de topo** do navegador (atributo `popover="manual"`, posição na janela
  calculada em `lib/placeBubble.ts` e refeita a cada rolagem), então nenhum
  `overflow` nem o `backdrop-filter` do vidro o corta, nem na tabela que rola;
  sem `popover` (navegador velho, jsdom) o balão fica no fluxo como antes;
  (2) a `DataTable` só rola de lado (`data-scrolls`) quando a tabela de fato
  não cabe, medida por `ResizeObserver`: a área de toque de 44 px do ⓘ no canto
  da tabela não cria mais uma rolagem fantasma de 2 px, e o `overflow: visible`
  local do `analysis.css` saiu (ele deixava a tabela larga e o texto só de leitor
  de tela alargarem a **página** em 768–820 px).
- **Impostos.** A tabela de meses tinha 764 px para 641–690 px. O vencimento foi
  para baixo do DARF e o recheio lateral das células ficou em um passo menor.
- **Metas no celular (1 px).** A suspeita do `::after` do ⓘ estava certa pela
  metade: a fileira de três perfis e o ⓘ passava 9 px da largura do cabeçalho, e
  a área de toque do ⓘ chegava a 1 px além da janela. A fileira encolhe.
- **Menu lateral (84 px).** "Desempenho" tem 73 px e o item tinha 62 px. O recheio
  do trilho e do item caiu; na barra de baixo do celular, abaixo de 380 px, letra
  e recheio cedem.
- **Consistência.** Eyebrow repetido sai de Metas, Critérios, Objetivos,
  Importar da B3 e Proventos automáticos (a casca de Ajustes já situa); o botão
  primário do cabeçalho tem 44 px em toda tela; as pílulas de Ajustes quebram em
  duas linhas no iPad em pé em vez de esconder as últimas; as tabelas anuais de
  Renda e Desempenho cabem em 1024 px.
- **Medida.** O `measure-layout.mjs` passou a apontar rolagem lateral (da página,
  da folha e de blocos internos) e aceita `MEASURE_THEME=light`.
