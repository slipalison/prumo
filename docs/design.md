# Desenho da interface

## A ideia em uma frase

**Uma pergunta por tela.** A tela de abertura pergunta "quanto você vai aportar?"
e responde com o que comprar. Todo o resto — carteira, metas, critérios — existe
para essa resposta ficar certa, e fica a um toque de distância, nunca no caminho.
São três itens na navegação: **Aportar**, **Carteira** e **Ajustes** (onde ficam
as metas e os critérios).

## Identidade: o fio de prumo

O prumo é o peso na ponta de um fio que mostra a vertical verdadeira. É a
metáfora do produto: a carteira **balança**, e o aporte certo a traz de volta ao
eixo.

- **Marca:** um peso de prumo (gota invertida facetada) pendurado num fio fino.
- **Animação-assinatura:** enquanto calcula, o prumo **balança** e vai
  amortecendo até parar no centro (mola amortecida, ~1,2 s). Parou = carteira no
  eixo. É o loading do cálculo, a animação do vazio e o selo de sucesso.
- **Marcador de meta:** nas barras de cada classe, a meta é uma linha vertical
  fina com um pequeno prumo no topo — o "eixo" daquela classe.

## Atmosfera

Escura por padrão, com tema claro completo (a escolha da pessoa vence o
sistema; "sistema" remove o atributo e deixa a media query mandar).

- **Fundo aurora:** três a quatro manchas de gradiente (índigo, teal, violeta,
  um toque de âmbar) muito desfocadas, derivando devagar (40–60 s por ciclo),
  sobre um quase-preto azulado. Por cima, ruído granulado sutil (SVG
  `feTurbulence`, opacidade ~4%) para tirar o aspecto de gradiente digital.
- **Vidro:** cartões com `backdrop-filter: blur(20px) saturate(140%)`, fundo
  translúcido, borda de 1px em gradiente (máscara) que acende onde o cursor
  passa (spotlight radial que segue o ponteiro por variáveis CSS).
- **Dourado = dinheiro.** O acento principal é um âmbar/ouro quente; aparece no
  número do aporte, no botão principal e no que vai ser comprado. Nunca como
  decoração gratuita.
- **Tipografia:** Inter Variable para texto; Space Grotesk Variable para
  números grandes e títulos (algarismos tabulares nos valores). Fontes
  empacotadas (`@fontsource-variable/*`), nada de CDN.

### Cores das classes

Sete tons distintos, legíveis sobre o fundo escuro e o claro, sempre
acompanhados de nome (cor nunca é a única pista):

| Classe | Tom |
|---|---|
| Ações Nacionais | verde-esmeralda |
| Fundos Imobiliários | âmbar |
| Ações Internacionais | azul-céu |
| REITs | rosa-orquídea |
| Criptomoedas | violeta |
| Renda Fixa | laranja-cobre |
| Renda Fixa Internacional | lavanda |

## Movimento

Biblioteca `motion` (Framer Motion). Molas em vez de curvas de tempo; nada dura
mais que 600 ms, exceto o prumo e o fundo.

- **Números contam** até o valor (count-up com mola) sempre que mudam.
- **Preço ao vivo:** o valor que andou pisca na cor do sentido (verde sobe,
  vermelho desce) e uma seta ▲/▼ aparece ao lado por 1,6 s — a seta diz o
  sentido sem depender da cor. No patrimônio, só a seta: o número já conta.
- **Rosca (donut)** da carteira: os arcos se desenham (`pathLength`) ao entrar;
  no resultado do aporte, a rosca **transita** do antes para o depois.
- **Barras de classe:** a parte atual cresce da esquerda; a parte que o aporte
  acrescenta aparece em listras douradas animadas ("vai entrar"), até o marcador
  de meta.
- **Cartões do plano** entram em cascata (stagger 60 ms), cada um com mola e um
  leve blur→nítido. O valor de cada um conta até o final.
- **Botão principal** magnético (desloca alguns pixels em direção ao cursor),
  brilho que atravessa a superfície no hover, compressão ao clicar.
- **Registrar aporte:** confete dourado (`canvas-confetti`) a partir do botão +
  check desenhado; as barras da carteira já aparecem atualizadas.
- **Abas:** indicador que desliza entre elas (`layoutId`); telas entram com fade
  + 8px de deslocamento (`AnimatePresence`).
- **Toggles do checklist:** pílulas grandes com botão de mola; a nota do ativo
  vira (flip) a cada resposta.
- **Esqueletos** com brilho deslizante enquanto carrega.
- **`prefers-reduced-motion`:** `MotionConfig reducedMotion="user"`; o fundo para
  de derivar, o prumo aparece parado e o confete não dispara.

## Telas

Celular primeiro. No celular, barra de abas embaixo; no desktop (≥ 960px), um
trilho à esquerda e conteúdo centralizado (máx. ~1100px).

### Navegação

- A barra lista as telas de `APP_ROUTES` (`frontend/src/app/routes.tsx`): tela
  nova é **uma linha** nessa lista, e a barra (que reparte a largura entre
  quantas houver), o título da aba, o `aria-current` e o roteamento saem dela.
  Hoje: Aportar, Carteira, Ajustes.
- Ajustes agrupa **seções** (`SETTINGS_SECTIONS`,
  `frontend/src/features/settings/sections.ts`): Metas por classe e Critérios.
  Seção nova também é uma linha. A lista de seções fica no topo de Ajustes
  (links com `aria-current="page"` na atual), cada seção tem endereço próprio
  (`#/ajustes/metas`, `#/ajustes/criterios`) e uma seção por vez na tela.
- Os links antigos `#/metas` e `#/criterios` seguem valendo: abrem a seção
  certa de Ajustes e trocam o endereço pelo novo, sem entrada nova no
  histórico (`LEGACY_ROUTES` em `lib/route.ts`).
- No canto (junto do tema; na barra do desktop) ficam o **olho** e o tema.

### 1. Aportar (abertura)

1. Pergunta grande: **"Quanto você vai aportar?"**
2. Campo de valor enorme, em reais, com máscara (digitar `600000` mostra
   `R$ 6.000,00`), foco automático.
3. Chips rápidos: R$ 500 · R$ 1.000 · R$ 2.000 · R$ 5.000 · último aporte.
4. Botão **Calcular** (Enter também calcula).
5. Resultado, abaixo, sem trocar de tela:
   - Manchete: "Compre **5 ativos** com **R$ 5.999,97**" (+ troco).
   - Cartões: ticker em destaque, nome, "**12 cotas** × R$ 53,81", valor.
   - Rosca antes → depois e as barras por classe com o pedaço dourado.
   - Avisos em linguagem simples ("Ações Internacionais está abaixo da meta,
     mas você não tem nenhum ativo lá — R$ 37.800 ficaram sem destino").
   - Botão **"Já comprei"** → registra o aporte (com chance de ajustar
     quantidades antes) → confete.
6. Carteira vazia: o prumo balançando e um convite de um toque para montar a
   carteira (assistente de três passos: perfil → metas, primeiro ativo, pronto).

### 2. Carteira

- Topo: patrimônio total (count-up) e a rosca por classe. Embaixo do total,
  de onde vem o número: **● Ao vivo** (ponto que pulsa) com o aviso "B3 com
  15 min de atraso" quando a carteira tem papel da B3; "Conectando ao vivo…"
  enquanto abre; fora do ao vivo, a hora da última cotação. O botão de
  atualizar continua ao lado.
- Uma seção por classe, na ordem fixa: nome, % atual vs meta com barra e
  marcador de prumo, e a lista dos ativos (ticker, quantidade, valor, nota).
- **Atual × ideal por ativo.** Sob cada ativo que recebe aporte (nota > 0 e
  preço), uma barra fina com um traço no ideal, e o texto "25,3% da classe ·
  ideal 30,0%". O ideal é a fatia `nota ÷ soma das notas` que o motor usa
  (`ideal_class_percent`); o "atual" é o quanto o ativo vale entre os ativos
  da classe que recebem. Abaixo do ideal por mais de 0,5 ponto, aparece o selo
  **↓ abaixo do ideal** (seta e texto, nunca só cor). Ativo sem nota ou sem
  preço não tem ideal nem selo: mostra só "% da classe" e o aviso que já tinha.
  No topo da tela, "N ativos abaixo do ideal" (ou "Nenhum ativo abaixo do
  ideal"); a linha não aparece sem nenhum ativo que receba. Tudo acompanha o
  preço ao vivo, porque o evento `prices` leva o ideal.
- Botão flutuante **+** abre uma folha (sheet) com **um campo só**: digita o
  ticker, aparece o autocompletar (`/symbols`), escolhe, a classe já vem
  sugerida em chips (troca com um toque), informa a quantidade, pronto. Para
  renda fixa: nome + valor, ou "Tesouro Direto" (busca pelo nome, quantidade
  fracionária, preço automático).
- Tocar num ativo abre a folha de edição: quantidade com stepper, preço manual
  opcional, o checklist (perguntas com sim/não/sem resposta) e a nota ao vivo;
  ou o controle de nota manual (0–10) para cripto e renda fixa. Remover fica no
  fim, discreto, com confirmação na própria folha (nunca `window.confirm`).

### 3. Ajustes › Metas por classe

- Perfis prontos em cartões (Conservador, Moderado, Arrojado) que preenchem as
  metas com animação.
- Um controle deslizante por classe, com a cor da classe.
- Anel de soma: mostra o total; fica verde e "estala" (pulso) ao chegar em
  100%. Botão **Salvar** só habilita em 100%; botão **Equilibrar** distribui o
  que falta/sobra proporcionalmente.

### 4. Ajustes › Critérios

- Duas listas (Ações e Imobiliário): perguntas editáveis no lugar, adicionar,
  remover, restaurar padrão.
- Texto curto explicando a nota: "cada sim soma 1, cada não tira 1".

### 5. Saúde

Avisos, não nota. Cabeçalho "Saúde da carteira" e uma frase dizendo que as
regras são da pessoa.

- **Veredito** (cartão no topo, `role="status"`): o prumo parado e
  "Nenhum aviso" quando está tudo dentro dos limites; "N avisos" com quantas
  regras passaram; ou "Ainda sem o que medir" (carteira vazia, sem metas).
  Nunca "tudo certo" quando não há o que medir.
- **Um cartão por aviso**, com a pergunta que o servidor respondeu em quatro
  linhas: o que é ("ITUB4 é 31,04% da carteira"), **Agora** e **Limite** em
  números grandes, a barra com o limite no fio de prumo (mesmo desenho da barra
  de classe, com o fio no limite em vez da meta) e **"O que fazer."** —
  aportar fora do que passou (valor em reais), nunca vender — com um link para
  Aportar ou Carteira. A barra é só reforço: o texto diz "passou".
- **Suas regras**: uma linha por regra, em frase ("Nenhum ativo acima de [15] %
  da carteira"), o campo do limite, o porquê em uma linha, o **estado em texto
  com ícone** (Passou do limite / Dentro do limite / Sem o que medir /
  Desligada) e o interruptor. "Voltar para 60%" aparece quando o limite não é o
  de fábrica. **Salvar regras** só habilita com mudança válida; o erro de um
  limite aparece na própria linha (`role="alert"`, `aria-invalid`).
- **De onde vem o setor**: a fonte de cada classe, de quando é, se a última
  busca falhou, quantos ativos ficaram sem setor e por que os EUA não têm.

Na **Carteira**:

- Abaixo do patrimônio, um **resumo curto** ("Saúde da carteira: 3 avisos", os
  dois primeiros e "e mais N") com o link "Ver painel". Some se os avisos não
  carregam, para nunca atrapalhar a carteira.
- Dentro de **Ações Nacionais** e **Fundos Imobiliários**, uma **rosca por
  setor** (por tipo de fundo nos FIIs) com a lista ao lado: nome e fatia sempre
  escritos; a cor só ajuda a achar. Sete setores têm cor; o resto vira "Outros
  setores", e o que a fonte não conhece vira "Sem setor" (cinza). Se a fonte
  nunca respondeu, uma linha diz isso no lugar da rosca.

A paleta dos setores (`--sector-1` a `--sector-7`, `--sector-other`,
`--sector-none`) tem tom próprio nos dois temas e fica em `styles/health.css`;
todos passam de 3:1 sobre o fundo (elementos gráficos), e o texto de cada fatia
está na lista.

### Histórico

Dentro de Carteira (ou do Aportar): lista dos aportes registrados com data,
valor e itens.

## Ocultar valores

O **olho** (botão de alternância com `aria-pressed`, na barra de navegação,
junto do tema) mascara os valores em todas as telas e fica salvo no
`localStorage` (`prumo.hide-values`).

- O ponto central é o `PrivacyProvider`: toda tela formata dinheiro e
  quantidade por `useMoney()` (`brl`, `compact`, `quantity`), e nenhuma decide
  sozinha. Com o olho fechado, esses formatadores devolvem `R$ ••••` e `••`.
- Vale para Carteira, Aportar (plano, troco, frases dos avisos, "último
  aporte"), histórico, folhas de edição e de registro, o patrimônio (o
  `CountUp` não conta número nenhum) e a rosca do plano.
- O leitor de tela lê a máscara: não há `aria-label` nem texto `sr-only` com o
  valor, e o `CountUp` só tem texto escondido quando está contando.
- **Campos de valor** (aporte, quantidade, valor atual, preço manual) seguem
  editáveis: fora do foco mostram a máscara; com o foco mostram o número, para
  a pessoa ver o que digita — o campo do aporte abre vazio, então nada vaza ao
  abrir. Os botões − e + da quantidade continuam funcionando às cegas.
- O **sentido** do preço também é dado: com o olho fechado somem as setas ▲/▼ e
  o brilho verde/vermelho. O selo "Ao vivo" fica, porque diz só o estado.
- Ficam visíveis o que não é valor em reais: percentuais, notas, nomes e
  tickers, e os atalhos fixos do campo do aporte (R$ 500, R$ 1.000...).

## Acessibilidade

- Contraste mínimo 4,5:1 para texto nos dois temas.
- Todo controle alcançável por teclado, foco visível (anel dourado).
- Folhas são `dialog` com foco preso e `Esc` fecha.
- Números com `aria-live="polite"` no resultado do aporte.
- O preço ao vivo **não** é anunciado (mudaria a cada segundo); o selo de
  estado é `role="status"` e só fala quando o estado muda.
- O selo "abaixo do ideal" traz seta e texto; a barra de atual × ideal é só
  desenho (`aria-hidden`), e o texto ao lado diz os dois números.
- Na navegação e nas seções de Ajustes, a atual leva `aria-current="page"`.
- Alvos de toque ≥ 44px.

## Texto

Português do Brasil, frases curtas, sem jargão. Valores em `pt-BR` (`R$ 1.234,56`).
Erros dizem o que fazer, não o código.
