# Desenho da interface

## A ideia em uma frase

**Uma pergunta por tela.** A tela de abertura pergunta "quanto você vai aportar?"
e responde com o que comprar. Todo o resto — carteira, metas, critérios — existe
para essa resposta ficar certa, e fica a um toque de distância, nunca no caminho.

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

- Topo: patrimônio total (count-up) e a rosca por classe.
- Uma seção por classe, na ordem fixa: nome, % atual vs meta com barra e
  marcador de prumo, e a lista dos ativos (ticker, quantidade, valor, nota).
- Botão flutuante **+** abre uma folha (sheet) com **um campo só**: digita o
  ticker, aparece o autocompletar (`/symbols`), escolhe, a classe já vem
  sugerida em chips (troca com um toque), informa a quantidade, pronto. Para
  renda fixa: nome + valor.
- Tocar num ativo abre a folha de edição: quantidade com stepper, preço manual
  opcional, o checklist (perguntas com sim/não/sem resposta) e a nota ao vivo;
  ou o controle de nota manual (0–10) para cripto e renda fixa. Remover fica no
  fim, discreto, com confirmação na própria folha (nunca `window.confirm`).

### 3. Metas

- Perfis prontos em cartões (Conservador, Moderado, Arrojado) que preenchem as
  metas com animação.
- Um controle deslizante por classe, com a cor da classe.
- Anel de soma: mostra o total; fica verde e "estala" (pulso) ao chegar em
  100%. Botão **Salvar** só habilita em 100%; botão **Equilibrar** distribui o
  que falta/sobra proporcionalmente.

### 4. Critérios

- Duas listas (Ações e Imobiliário): perguntas editáveis no lugar, adicionar,
  remover, restaurar padrão.
- Texto curto explicando a nota: "cada sim soma 1, cada não tira 1".

### Histórico

Dentro de Carteira (ou do Aportar): lista dos aportes registrados com data,
valor e itens.

## Acessibilidade

- Contraste mínimo 4,5:1 para texto nos dois temas.
- Todo controle alcançável por teclado, foco visível (anel dourado).
- Folhas são `dialog` com foco preso e `Esc` fecha.
- Números com `aria-live="polite"` no resultado do aporte.
- Alvos de toque ≥ 44px.

## Texto

Português do Brasil, frases curtas, sem jargão. Valores em `pt-BR` (`R$ 1.234,56`).
Erros dizem o que fazer, não o código.
