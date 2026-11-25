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
  Hoje: Aportar, Carteira, Renda, Desempenho, Ajustes.
- Ajustes agrupa **seções** (`SETTINGS_SECTIONS`,
  `frontend/src/features/settings/sections.ts`): Metas por classe, Critérios,
  Objetivos, Saúde da carteira, Importar da B3 e Impostos.
  Seção nova também é uma linha. A lista de seções fica no topo de Ajustes
  (links com `aria-current="page"` na atual), cada seção tem endereço próprio
  (`#/ajustes/metas`, `#/ajustes/criterios`, `#/ajustes/objetivos`, `#/ajustes/saude`,
  `#/ajustes/importar-b3`, `#/ajustes/impostos`) e uma seção por vez na tela.
- Os links antigos `#/metas` e `#/criterios` seguem valendo: abrem a seção
  certa de Ajustes e trocam o endereço pelo novo, sem entrada nova no
  histórico (`LEGACY_ROUTES` em `lib/route.ts`).
- No canto (junto do tema; na barra do desktop) ficam o **olho** e o tema.

### 1. Aportar (abertura)

1. Pergunta grande: **"Quanto você vai aportar?"**
2. Campo de valor enorme, em reais, com máscara (digitar `600000` mostra
   `R$ 6.000,00`), foco automático.
   - **Caixa**, logo abaixo do campo, quando há saldo: "+ R$ 812 em caixa —
     **Incluir**". Incluir soma o saldo ao valor digitado; "Tirar" desfaz; digitar
     um valor menor que o caixa incluído desfaz a inclusão (o caixa nunca passa
     do aporte). Um detalhe recolhido, "De onde vem esse caixa", diz quanto vem
     de proventos recebidos e de vendas, quanto os aportes já usaram, desde que dia
     a conta vale e os movimentos mais novos, cada um com o sinal escrito. A frase
     do detalhe lembra que, para aportar dinheiro de fora, basta digitar o valor
     total: só o que se incluiu do caixa sai dele. Ao registrar ("Já comprei"), a
     folha avisa "R$ X deste aporte saem do caixa" e o saldo baixa.
3. Chips rápidos: R$ 500 · R$ 1.000 · R$ 2.000 · R$ 5.000 · último aporte.
4. Botão **Calcular** (Enter também calcula).
5. Resultado, abaixo, sem trocar de tela:
   - Manchete: "Compre **5 ativos** com **R$ 5.999,97**" (+ troco).
   - Cartões: ticker em destaque, nome, "**12 cotas** × R$ 53,81", valor.
   - Rosca antes → depois e as barras por classe com o pedaço dourado.
   - Avisos em linguagem simples ("Ações Internacionais está abaixo da meta,
     mas você não tem nenhum ativo lá — R$ 37.800 ficaram sem destino"), cada
     um com a ação que resolve. "XPTO3 está a R$ 32,50, acima do seu teto de
     R$ 30,00" abre a folha do ativo, onde o teto está; "XPTO3 chegou no seu
     limite de 15% da carteira" (e a classe bloqueada pelo limite) leva ao
     controle do limite.
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
- **Lucro total**, logo abaixo do patrimônio: "Lucro total +R$ 3.150" — o ganho
  de capital (realizado + não realizado) mais os proventos recebidos. Um
  detalhe recolhido, "Como se chega nesse número", abre as três parcelas. O
  sinal é escrito (`+` / `−`), nunca só cor, e anda com o preço ao vivo (o evento
  `prices` leva o bloco). Quando algum ativo está sem preço médio, sem cotação ou
  com uma venda de custo desconhecido, o rótulo vira "Lucro total (parcial)" e o
  detalhe diz quem falta ("Parcial: falta o preço médio de HGLG11 …"): o número
  é só do que se sabe, nada é estimado. Venda de ativo que já saiu da carteira
  tem aviso próprio.
- Uma seção por classe, na ordem fixa: nome, % atual vs meta com barra e
  marcador de prumo, e a lista dos ativos (ticker, quantidade, valor, nota).
- **Atual × ideal por ativo.** Sob cada ativo que recebe aporte (nota > 0 e
  preço), uma barra fina com um traço no ideal, e o texto "25,3% da classe ·
  ideal 30,0%". O ideal é a fatia `nota ÷ soma das notas` que o motor usa
  (`ideal_class_percent`); o "atual" é o quanto o ativo vale entre os ativos
  da classe que recebem. Abaixo do ideal por mais de 0,5 ponto, aparece o selo
  **↓ abaixo do ideal** (seta e texto, nunca só cor). Com o preço acima do
  teto da pessoa, ao lado aparece o selo **⚠ acima do teto** (ícone e texto, em
  tom neutro): o ideal não muda por causa do teto, e o selo explica por que o
  ativo não recebe agora. Ativo sem nota ou sem
  preço não tem ideal nem selo: mostra só "% da classe" e o aviso que já tinha.
  No topo da tela, "N ativos abaixo do ideal" (ou "Nenhum ativo abaixo do
  ideal"); a linha não aparece sem nenhum ativo que receba. Tudo acompanha o
  preço ao vivo, porque o evento `prices` leva o ideal.
- Na linha do ativo, a **rentabilidade** fica embaixo do valor: `+9,2%` /
  `−3,0%`, em verde ou vermelho, com o **sinal escrito** (cor nunca sozinha; a
  seta ▲/▼ continua sendo só do preço que acabou de andar, para as duas coisas
  não se confundirem). Sem ideal, ela divide a linha com o "% da classe"; com
  ideal, o "% da classe" já está na linha do ideal embaixo e a rentabilidade
  fica sozinha sob o valor — cada número aparece uma vez. O **preço médio**
  entra na coluna de quantidade ("10 cotas · R$ 142,00 · PM R$ 130,00"), que
  só existe do desktop (≥ 720px) para cima: no celular a linha fica com
  valor, rentabilidade e o ideal, e o PM está na folha do ativo. Sem custo
  conhecido, nem PM nem rentabilidade aparecem — nada de "0%" que não é
  verdade. A rentabilidade anda com o preço ao vivo.
- Botão flutuante **+** abre uma folha (sheet) com **um campo só**: digita o
  ticker, aparece o autocompletar (`/symbols`), escolhe, a classe já vem
  sugerida em chips (troca com um toque), informa a quantidade, pronto. Para
  renda fixa: nome + valor, ou "Tesouro Direto" (busca pelo nome, quantidade
  fracionária, preço automático).
- Tocar num ativo abre a folha de edição: quantidade com stepper, preço manual
  opcional, **teto de preço** opcional, os **Lançamentos**, o checklist (perguntas com sim/não/sem
  resposta) e a nota ao vivo; ou o controle de nota manual (0–10) para cripto e
  renda fixa.
- Teto de preço: uma chave "Teto de preço" com a explicação curta ("Seu
  critério: acima deste preço o ativo fica fora do aporte até voltar. Não é
  preço justo de ninguém.") e, ligada, o campo "Pagar no máximo, por unidade".
  Com o preço de hoje acima do teto digitado, um aviso com ícone e texto diz
  "Hoje está a R$ X, acima do teto: fica fora do aporte". Renda fixa por valor
  não tem a chave. Remover fica no
  fim, discreto, com confirmação na própria folha (nunca `window.confirm`).
- **Lançamentos**, dentro da folha:
  - um resumo em quatro números — preço médio, custo (em renda fixa,
    "aplicado"), resultado com a rentabilidade e lucro realizado —, "—" onde
    o custo é desconhecido; um quinto, **yield on cost (12 meses)**, aparece só
    para o ativo que rendeu provento (nada de "0,0%" em quem não paga): o
    percentual, "(estimado)" quando não há recebido, ou "informe o preço médio"
    quando falta o custo;
  - custo desconhecido mostra o aviso dourado "Informe o preço médio para ver
    a rentabilidade" com o botão que abre o saldo inicial já com o foco no
    preço;
  - a lista, do mais novo para o mais antigo: tipo (com a etiqueta "do aporte"
    ou "de antes do livro"), quantidade × preço, taxa e data; editar e apagar
    por ícone, apagar com confirmação na própria linha;
  - adicionar abre o formulário no lugar: tipo, data (hoje por padrão, nunca
    futura), quantidade e, conforme o tipo, preço, taxas e — em ativo cotado
    em dólar — moeda e câmbio. Cada tipo diz o que a quantidade é
    (desdobramento: "as cotas novas que você recebeu"). Renda fixa troca os
    nomes (aplicação, resgate, atualização do saldo) e não pede preço;
  - salvar refaz a posição no servidor; a quantidade da folha acompanha o
    livro. Venda maior que a posição numa data volta com a frase que diz o
    que conferir, sem fechar o formulário.
- A quantidade editada na própria folha vira um "ajuste" em Lançamentos (a
  dica sob o campo diz isso); compra e venda vão em Lançamentos, com data e
  preço.
- No cadastro, com quantidade, um campo opcional de **preço médio** (renda
  fixa: "quanto você aplicou"). Sem ele, o ativo entra e a tela pede depois.

### 3. Ajustes › Metas por classe

- Perfis prontos em cartões (Conservador, Moderado, Arrojado) que preenchem as
  metas com animação.
- Um controle deslizante por classe, com a cor da classe.
- Anel de soma: mostra o total; fica verde e "estala" (pulso) ao chegar em
  100%. Botão **Salvar** só habilita em 100%; botão **Equilibrar** distribui o
  que falta/sobra proporcionalmente.
- Embaixo, o cartão **Limite por ativo**, com o próprio **Salvar limite** (não
  depende das metas fecharem 100%): uma chave "Limitar a concentração" que diz
  o estado salvo ("Desligado: cada ativo recebe pela nota, sem teto." / "Ligado
  em 15% da carteira.") e, ligada, um controle deslizante de 1% a 100% (começa
  em 20%). O texto explica que o que passaria vai para os outros ativos da
  classe e que o motor nunca vende. Os avisos do plano sobre o limite levam
  para cá (`#/ajustes/metas`).

### 4. Ajustes › Critérios

- Duas listas (Ações e Imobiliário): perguntas editáveis no lugar, adicionar,
  remover, restaurar padrão.
- Texto curto explicando a nota: "cada sim soma 1, cada não tira 1".

### 5. Saúde (seção de Ajustes)

Em `#/ajustes/saude`, fora da barra principal: a pessoa chega por Ajustes ou
pelo resumo da Carteira. Avisos, não nota. Cabeçalho "Saúde da carteira" e uma frase dizendo que as
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
- **Avisos de renda** (`income_every_month`, `max_payer`) usam o mesmo cartão.
  O de meses traz "Meses sem provento: fev/2026, abr/2026" e, em "O que fazer.",
  quem da carteira já pagou naquele mês (sem barra: o limite é uma contagem de
  meses, "Mínimo"); o de pagador traz a barra e, no texto, os reais que ele pagou
  e o total, que o olho fechado mascara. Os percentuais e os meses ficam à vista.
- **De onde vem o setor**: a fonte de cada classe, de quando é, se a última
  busca falhou, quantos ativos ficaram sem setor e por que os EUA não têm.

Na **Carteira**:

- Abaixo do patrimônio, um **resumo curto** ("Saúde da carteira: 3 avisos", os
  dois primeiros e "e mais N") com o link "Ver painel" para `#/ajustes/saude`.
  Junto com o "N ativos abaixo do ideal" do cabeçalho, são os dois resumos do
  topo: o do ideal diz onde aportar, o de saúde diz o que passou do limite. Some
  se os avisos não carregam, para nunca atrapalhar a carteira.
- Dentro de **Ações Nacionais** e **Fundos Imobiliários**, uma **rosca por
  setor** (por tipo de fundo nos FIIs) com a lista ao lado: nome e fatia sempre
  escritos; a cor só ajuda a achar. Sete setores têm cor; o resto vira "Outros
  setores", e o que a fonte não conhece vira "Sem setor" (cinza). Se a fonte
  nunca respondeu, uma linha diz isso no lugar da rosca.

Com o **olho fechado**, o aporte em reais de cada aviso vira `R$ ••••` (por
`useMoney()`, também para o leitor de tela); percentuais, limites e as fatias
das roscas são percentuais e ficam à vista. O título do Tesouro Direto (renda
fixa) não tem setor e aparece nos avisos pelo nome ("Tesouro IPCA+ 2035"), não
pela chave `TD-…`.

A paleta dos setores (`--sector-1` a `--sector-7`, `--sector-other`,
`--sector-none`) tem tom próprio nos dois temas e fica em `styles/health.css`;
todos passam de 3:1 sobre o fundo (elementos gráficos), e o texto de cada fatia
está na lista.
### 6. Desempenho

Na barra principal, depois de Carteira.

Responde "estou ganhando do CDI?". Método e limites em
[`desempenho.md`](desempenho.md).

- Chips de período no topo: Mês · No ano · 12 meses · Desde o início.
- Dois cartões lado a lado (empilhados no celular): **Últimos 12 meses** e
  **Desde o início**, com a rentabilidade grande e assinada (`+1,23%`), a frase
  "**1,13% acima do CDI** (118% do CDI). O CDI rendeu +0,10%." e as datas. Se a
  série ainda não tem 12 meses, o cartão diz.
- **Patrimônio no tempo:** patrimônio, aplicado, ganho e proventos; área
  dourada do patrimônio e a linha tracejada neutra do aplicado. Uma nota diz
  que o ganho é o do preço e que os proventos entram na rentabilidade. O ponto
  de hoje antes do fechamento tem o selo "Hoje é parcial".
- **Série longa:** a série começa no primeiro lançamento do livro (anos, depois
  de importar a B3). As datas das pontas do eixo e o resumo do leitor de tela
  levam o ano quando a série passa de um ano.
- **Série sendo refeita:** depois de importar ou de mexer num lançamento
  antigo, uma nota (`role="status"`) diz "Refazendo a série desde DD/MM/AAAA
  com os lançamentos novos. Os números se acertam em instantes." e a tela
  pergunta de novo a cada 5 s até acabar.
- **Rentabilidade contra os índices:** a linha da carteira e uma por índice.
  A legenda mostra o número de cada um no período e liga/desliga os índices
  (`aria-pressed`). Ao passar o ponteiro (ou tocar), uma régua vertical marca o
  dia e uma caixa lista o valor de cada linha. Índice sem dado no período
  aparece como "sem dado", nunca como zero; uma nota diz até quando cada
  índice foi publicado.
- **Mês a mês:** tabela com os anos (o mais novo em cima), os doze meses, o ano
  e o CDI do ano. É também a alternativa ao gráfico para leitor de tela (o
  gráfico é `role="img"` com um resumo em texto).
- **Gráficos:** SVG próprio, linha de 2px que se desenha ao entrar
  (`pathLength` com mola), grade discreta, zero marcado na rentabilidade. Cores
  das linhas, validadas para daltonismo e contraste ≥ 3:1 nos dois temas:
  carteira âmbar (o dourado do dinheiro), CDI azul, IPCA laranja, Ibovespa
  violeta — e cada índice com o próprio tracejado, para a cor não ser a única
  pista. Texto nunca usa a cor da série. Alta e baixa levam sinal (+/−) além
  do verde/vermelho.
- **Olho fechado:** todo valor em R$ (patrimônio, aplicado, ganho, proventos, o eixo e a
  régua do gráfico de patrimônio e o resumo para o leitor de tela) sai por
  `useMoney()` como `R$ ••••`, e o ganho perde o sinal e a cor. As
  rentabilidades são percentuais e ficam à vista, como na Carteira.
- **Estado vazio honesto:** sem lançamento, "Ainda não há série" com o caminho
  para a Carteira e para a importação da B3; com uma foto só, "A série começa em DD/MM/AAAA" e nenhum
  número inventado.

### 7. Ajustes › Importar da B3

- Passo a passo em texto de como baixar as planilhas na Área do Investidor (sem
  imagem de terceiros) e o aviso do que a planilha não traz (corretagem).
- Seletor de um ou mais `.xlsx`; cada arquivo da lista mostra o que foi lido
  (tipo, linhas, quantas entram, quantas não foram entendidas) ou, se não abriu,
  o motivo em português.
- Prévia: contadores do que foi lido e, contra a carteira, quantas linhas
  **entram**, **já existem**, **conflitam** e **ficam de fora**. Tabela
  agrupada por ativo: classe (num ativo novo, um seletor com a sugestão; no que
  já está na carteira, a classe dele), a situação ("entra na carteira", "já
  está na carteira", "nada novo", "deixado de fora", com os selos "conflito" e
  "confira a classe"), compras, vendas, saldo dos negócios e proventos. Cada
  linha que expande diz o destino dela: "— entra", "— já existe: é o
  lançamento que você já fez", "— fica de fora: a receber sem data"...
- **Conferir antes de importar:** um bloco por ativo que pede atenção —
  classe só pelo código (botão "Confirmar Fundos Imobiliários"), saldo inicial
  sugerido (data, quantidade, preço médio ou "custo desconhecido", o porquê, e
  os campos de quantidade e preço médio para trocar), os saldos e ajustes
  digitados que dão lugar ao histórico, "na carteira hoje → depois de
  importar", o conflito em texto da tela (nunca o do servidor, que tem
  número) e a chave "Deixar de fora desta importação".
- Mudou uma escolha: aparece **Atualizar prévia** e o **Importar** espera; a
  gravação usa o `digest` da prévia que está na tela. Importar desligado diz
  por quê (conflito ou classe por confirmar; "Nada novo: tudo já está na
  carteira"). Ligado, diz quantos lançamentos e proventos grava.
- Depois de gravar, a prévia dá lugar ao **resultado**: quantos lançamentos e
  proventos entraram (e quantos substituíram um a receber), ativos novos e
  atualizados, saldos iniciais, o que já existia e o que ficou de fora, por
  motivo. Carteira, lançamentos, Renda, caixa, saúde e Desempenho são lidos de
  novo; o resultado avisa que o Desempenho refaz a série em segundo plano.
  Prévia vencida (`409`) avisa e lê as planilhas de novo.
- É uma seção de Ajustes (`#/ajustes/importar-b3`). Quantidades e valores da
  prévia, da conferência e do resultado seguem o olho de ocultar valores,
  inclusive para o leitor de tela e nos campos fora do foco, e o texto cru das
  linhas não entendidas (que traz valores) também some. Contagem de linhas e
  de ativos fica à vista.

### 8. Renda

Aba entre Carteira e Desempenho (uma linha em `APP_ROUTES`). Responde "quanto minha carteira paga?" sem virar
planilha: uma meta no topo, o resto por baixo.

1. **Meta de renda mensal.** Número grande (média mensal dos 12 meses
   recebidos), barra de progresso até a meta (`role="progressbar"` com texto
   para o leitor de tela: "R$ 135 de R$ 800 por mês, 17%") e a data em que foi
   atingida, quando foi. Sem nada recebido ainda, a barra usa a **estimativa** e
   fica listrada, com a etiqueta "estimativa": o desenho muda, não só a cor.
2. **Três números:** recebido em 12 meses, média por mês e a receber.
3. **Mês a mês:** barras em SVG próprio com Motion (mola, como as barras de
   classe). O recebido é cheio e embaixo; o a receber é **listrado** e em cima;
   a meta é uma linha tracejada com o valor. A legenda dá nome a cada um e uma
   tabela escondida (`sr-only`) traz os 12 meses para o leitor de tela.
4. **Próximos pagamentos:** data, ativo, tipo e líquido.
5. **Estimativa da carteira de hoje:** "~R$ 410/mês" com a etiqueta
   "estimativa" e o aviso de que é o que a cota pagou nos últimos 12 meses, sem
   separar JCP e sem ser extrato. Papéis sem histórico são listados.
6. **Ano por mês:** tabela real (`<table>`), uma linha por ano.
7. **Yield on cost** da carteira, um cartão antes do mês a mês: o percentual
   grande e a conta por baixo ("Renda dos últimos 12 meses (recebida) ÷ o que
   você pagou. Hoje: R$ X sobre R$ Y"). Sem recebido, a renda é a estimada e o
   texto diz "(estimado)" e "estimada, não é extrato". Ativo sem preço médio sai
   da conta, e o cartão avisa "Parcial: fica de fora o que está sem preço médio
   (…)". Sem nenhum custo conhecido, no lugar do número vem "informe o preço médio".
8. **Quem pagou:** rosca (a mesma `Donut`) e a lista com nome, valor, parte e
   **yield on cost** de cada um ("yield on cost 6,2%" ou "informe o preço
   médio"); do sétimo em diante vira "Outros (N)", sem yield (junta vários).
9. **Proventos lançados:** lista com apagar (confirmação na própria linha).
10. **Lançar provento:** folha com ativo, tipo (chips), data de pagamento, valor
   por cota, quantidade e data ex opcional. **Em branco, a quantidade vale a do
   livro na data com** (não a de hoje): o campo mostra o número que o servidor
   vai usar ("Em branco vale o que o livro tinha na data com (09/11/2026)") e ele
   muda quando a data ex muda. Sem cotas naquela data, a folha avisa e só deixa
   lançar com a quantidade informada. A
   prévia mostra bruto, IR retido e líquido (JCP perde 15% até 2025 e 17,5%
   desde 2026, pela data de pagamento) e fala com
   `aria-live`. Renda fixa por valor pede só o valor recebido.

Na tela **Aportar**:

- Abaixo do campo de valor, quando há caixa: "+ R$ 812 em caixa —
  **Incluir**" (proventos recebidos e vendas, menos o que aportes anteriores já
  usaram; ver o item 1 de "Aportar"). Incluir soma ao valor; "Tirar" desfaz. Só
  mexe no valor de entrada.
- No resultado, uma frase com o selo "estimativa": "Este aporte soma ~R$ 35/mês
  de renda: sua renda vai de R$ 410 para R$ 445 por mês". Cada cartão mostra a
  renda mensal que ele soma. O plano não espera o provedor de dividendos: sem
  histórico em cache ainda, a frase vira "Estimativa de renda indisponível por
  enquanto".
- Com o olho fechado (ver "Ocultar valores"), tudo isso sai mascarado — meta,
  barras, tabelas, rosca, prévia do lançamento e a quantidade em branco (o que o
  livro tinha na data com), a renda do plano, o caixa (saldo, detalhe e
  movimentos), o lucro total e os valores do yield on cost —, inclusive o que só o leitor de tela lê (`aria-label`,
  `aria-valuetext`, tabelas `sr-only`). Só a proporção (percentual) continua — o yield on cost é percentual e fica; o
  motivo de um yield ausente ("informe o preço médio") e o "parcial" do lucro
  total também, porque são texto, não valor.

### 9. Ajustes › Objetivos

Seção de Ajustes (`#/ajustes/objetivos`, uma linha em `SETTINGS_SECTIONS`):
metas com prazo, de patrimônio total ou de valor numa classe. A meta de renda
mensal continua na tela Renda (a frase de abertura aponta para lá).

- **Em andamento**: um cartão por objetivo, com o nome ("Patrimônio total",
  "Valor em Fundos Imobiliários"), a meta, a barra de progresso
  (`role="progressbar"`, com "R$ 250 mil de R$ 1 mi, 25,0%" para o leitor de
  tela), a frase da data ("a meta chega em mar/2031 (daqui a 4 anos e 4
  meses)") e, **sempre à vista**, as duas premissas da conta em lista: "Aporte
  de R$ 2.400 por mês: a média das suas compras nos últimos 12 meses" e
  "Rentabilidade de 9,80% ao ano: a da sua carteira em Desempenho". Nenhuma data
  aparece sem a conta que a produziu. Sem aporte e sem rentabilidade positiva,
  a frase diz que a meta não chega e pede um aporte; depois de 100 anos, diz
  isso em vez de uma data.
- **Concluídos**: o cartão fica com a barra verde e "Concluído em 15/10/2026",
  sem premissas (não há mais conta). A cor nunca vai sozinha: o estado e a data
  estão escritos.
- **Novo objetivo / Editar** (folha): tipo (patrimônio ou classe), classe, meta
  e duas chaves, "Informar o meu aporte" e "Informar a minha rentabilidade".
  Desligadas, a folha diz o que a conta vai usar (a média do livro; o padrão de
  rentabilidade e de onde vem). Ligadas, abrem o campo; a rentabilidade vai de
  −50% a 100% ao ano. Na edição o tipo e a classe são texto fixo. Remover
  pede confirmação no próprio cartão.

Na tela **Aportar**, abaixo do resultado do plano, um cartão "Seus objetivos"
(só aparece com objetivo ativo; se a consulta falha, some e o plano segue):

- "Com este aporte, você chega a **25,6%** da meta de R$ 1.000.000 (hoje,
  25,0%)." Se o aporte fecha a meta: "você completa a meta de R$ …".
- "Com aportes assim, a meta chega em **fev/2031**." (com aporte informado:
  "Com aportes de R$ 1.500 por mês, …"). A data conta a partir do valor que a
  carteira passa a ter com o aporte.
- As mesmas duas premissas em lista e um link para Ajustes › Objetivos, onde se
  troca cada uma.
- Com o olho fechado, a meta e o aporte saem como `R$ ••••`, inclusive nos
  `aria-valuetext` e nos campos da folha; ficam o percentual, o mês estimado e
  os nomes.

### 10. Ajustes › Impostos

Seção de Ajustes (`#/ajustes/impostos`, uma linha em `SETTINGS_SECTIONS`).
Responde "devo DARF este mês?" e "o que vai na declaração?" sem esconder que é
estimativa. A regra e a ligação com o livro estão em `docs/ir.md`.

1. **Aviso de responsabilidade**, sempre visível no topo (`role="note"`), em
   faixa dourada: estimativa, não substitui o contador, confira antes de pagar
   ou entregar.
2. **Ano-calendário**: chips com os anos que têm regra (hoje 2025 e 2026). Abre
   no ano de hoje; sem regra para ele, no último com regra. Ano sem regra no
   servidor vira um cartão de erro com o motivo ("ainda não a de 2027"), nunca
   a conta de outro ano.
3. **Isentômetro** do mês de hoje (dezembro, num ano passado): o número grande
   das vendas de ações, "de R$ 20.000,00 isentos", a barra (a mesma `LimitBar`
   dos avisos, com o fio de prumo no limite e o percentual em texto) e a frase:
   dentro do limite, quanto ainda cabe; passou, "o lucro das ações neste mês
   paga 15%", em vermelho e escrito (a cor só reforça). Lembra que ETF, BDR e
   FII não têm essa isenção e, se houve, mostra a cripto contra o limite dela.
4. **Meses do ano**: tabela real (`<table>` em região com rolagem lateral),
   uma linha por mês: vendas de ações, imposto do mês, DARF (código e valor),
   vencimento e situação. "A pagar" (dourado) ou "Pago" (verde), com o botão
   "Marcar como pago"/"Desmarcar" (o nome acessível diz o código e o mês). DARF
   abaixo de dez reais aparece como "abaixo do mínimo, soma no mês seguinte".
   Se o valor mudou depois de marcado, a linha avisa. Embaixo, o prejuízo a
   compensar em 31/12 e, por mês com movimento, um `<details>` com a apuração
   por grupo (resultado, prejuízo antes, compensado, base, alíquota, imposto,
   prejuízo que fica) e a cripto.
5. **Declaração do ano**: Bens e Direitos (ticker, grupo, código, rótulo com
   acento, quantidade, situação em 31/12 pelo custo, preço médio) com
   **Copiar** por linha; rendimentos isentos (09, 99, 20, 05) com o valor por
   pagador; tributação exclusiva (JCP, líquido, bruto e retido); e a renda
   variável do ano (DARF do ano e o prejuízo que passa para o ano seguinte).
6. **O que o Prumo não calculou**: cada item do livro ou dos proventos que
   ficou fora, com ticker, data, valor e o motivo em uma frase; depois a lista
   fixa do que nunca entra (IR retido da nota, corretora, opções, exterior…).
7. **Pontos a confirmar com contador**: as dúvidas de `docs/ir.md` § 5 entram
   em caixa azul ao lado do número que elas mexem, nunca numa lista solta.
8. **Saldo de antes do Prumo**: o que foi informado (ou "a conta começa do
   zero em 1º de janeiro de 2025") e "Informar saldo"/"Editar saldo", que abre
   o ano (chips) e os seis campos de dinheiro.

Na **folha do ativo** (Carteira), abaixo do teto: "No imposto de renda", um
`<select>` com os sete instrumentos, o padrão marcado "(padrão)" e a frase do
que a escolha muda ("15% sobre o lucro, sem a isenção das ações…") e qual é o
padrão. Renda fixa por valor não tem a escolha.

Com o olho fechado, todo R$ e toda quantidade da seção (isentômetro, tabela,
apuração, declaração, não calculado, saldo) sai mascarado, inclusive no nome
acessível dos botões e da barra; percentuais, datas, códigos e tickers ficam. O
"Copiar" leva o valor de verdade para a área de transferência: é o que a pessoa
pediu, e a tela continua mascarada.

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
- Os avisos do teto e do limite também: o preço e o teto do "acima do seu
  teto" e o que ficou sem destino por causa do limite saem como `R$ ••••`; o
  limite em si (15%) é percentual e fica.
- **Campos de valor** (aporte, quantidade, valor atual, preço manual, teto de
  preço) seguem
  editáveis: fora do foco mostram a máscara; com o foco mostram o número, para
  a pessoa ver o que digita — o campo do aporte abre vazio, então nada vaza ao
  abrir. Os botões − e + da quantidade continuam funcionando às cegas.
- O **sentido** do preço também é dado: com o olho fechado somem as setas ▲/▼ e
  o brilho verde/vermelho. O selo "Ao vivo" fica, porque diz só o estado.
- **Lançamentos e posição**: preço médio, custo, resultado e lucro realizado
  em reais viram `R$ ••••` (sem sinal: o sentido fica na rentabilidade em %);
  na lista, a linha diz só o tipo, a data e "•• cotas × R$ ••••"; os campos
  do formulário seguem a regra dos campos de valor acima. Na linha da
  Carteira, "PM R$ ••••". O câmbio do lançamento é cotação pública e fica.
- Ficam visíveis o que não é valor em reais: percentuais (inclusive a rentabilidade da posição), notas, nomes e
  tickers, e os atalhos fixos do campo do aporte (R$ 500, R$ 1.000...).

## Acessibilidade

- Contraste mínimo 4,5:1 para texto nos dois temas.
- Todo controle alcançável por teclado, foco visível (anel dourado).
- Folhas são `dialog` com foco preso e `Esc` fecha.
- Números com `aria-live="polite"` no resultado do aporte.
- Gráficos de barras e roscas têm descrição (`role="img"` + `aria-label`) e uma
  tabela escondida ou a lista ao lado com todos os números; recebido e a receber
  se distinguem por desenho (cheio × listrado), nunca só por cor.
- O preço ao vivo **não** é anunciado (mudaria a cada segundo); o selo de
  estado é `role="status"` e só fala quando o estado muda.
- O selo "abaixo do ideal" traz seta e texto; a barra de atual × ideal é só
  desenho (`aria-hidden`), e o texto ao lado diz os dois números.
- Na navegação e nas seções de Ajustes, a atual leva `aria-current="page"`.
- Alvos de toque ≥ 44px.
- Cor nunca sozinha: os avisos do teto e do limite são frase com ícone; o
  estado da chave do limite também vem escrito. O controle do limite anuncia
  o valor como "15% da carteira" (`aria-valuetext`).
- Rentabilidade: o leitor de tela ouve "rentabilidade +9,2%" (o rótulo é
  texto `sr-only`); o sinal é texto, não só cor. Verde e vermelho são os
  `--success`/`--danger` dos temas, com contraste ≥ 4,5:1 nos dois.
- Lançamentos: cada campo com rótulo; o que muda o sentido da quantidade é
  dito no `aria-describedby` do campo; os botões de editar e apagar dizem qual
  lançamento ("Apagar Compra de 10/02/2026"); erros em `role="alert"`.

## Texto

Português do Brasil, frases curtas, sem jargão. Valores em `pt-BR` (`R$ 1.234,56`).
Erros dizem o que fazer, não o código.
