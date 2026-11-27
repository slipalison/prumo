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

### Ícone do ativo

Cada ativo, em qualquer lista (Carteira, plano e registro do aporte, Renda,
busca de símbolo, Análise, Impostos), leva um círculo à esquerda do nome
(`AssetIcon`, três tamanhos: 24, 36 e 44 px). O nome do ativo sempre está ao
lado, então o ícone é **enfeite**: `aria-hidden`, e a imagem leva `alt=""`.

- **Logo**, quando o servidor tem (ações e FIIs da B3, via brapi): um `<img>`
  de `/api/v1/icons/<símbolo>?v=<versão>` (mesmo domínio, CSP intacta), com
  `loading="lazy"`, recortado no círculo (`--r-pill`).
- **Iniciais do ticker** (`ITSA4` → IT; sem ticker, as do nome: `Caixa Reserva`
  → CR) no círculo **com a cor da classe**, quando não há logo. A tinta é
  `--icon-ink`: escura sobre as cores claras do tema escuro, branca sobre as
  cores escuras do tema claro; **4,5:1 ou mais** nas sete classes e no neutro,
  nos dois temas (conferido por teste, `lib/assetIcon.test.ts`). Sem como dizer a
  classe (lista que só traz o ticker e o ticker não ajuda), o círculo é neutro.
- **Desenhos próprios**, sem fonte externa, para o que não tem logo de empresa:
  Tesouro (um frontão com colunas), bitcoin (um "B" com dois traços) e cripto
  genérica (uma moeda com um losango), na cor da classe. O nome da marca vai
  como texto, nunca no desenho. O logo vence o desenho (um ETF de bitcoin da B3
  mostra o logo do gestor).
- **Não pisca**: o círculo de iniciais está sempre por baixo; o logo fica
  transparente até o `load` e só então aparece. Se a imagem falha, ela sai e
  ficam as iniciais (uma versão nova do logo tem nova chance).
- **Sem 404 em cascata**: a tela consulta `GET /icons` uma vez e só pede a
  imagem do que está na lista. Logo de ativo recém-adicionado chega em segundos
  e aparece quando a lista é pedida de novo (ao voltar para a aba ou ao abrir uma tela, se já passou 1 minuto da última).

## Movimento

Biblioteca `motion` (Framer Motion). Molas em vez de curvas de tempo, com os
tokens de `lib/motion.ts` e `--dur-*` em `styles/tokens.css` (os números e o
porquê em [layout.md](layout.md), § 6): nada que a pessoa espera passa de
~300 ms; ficam de fora o fundo, o prumo que balança, o esqueleto, o pulso do
"Ao vivo" e o lampejo do preço.

- **Números contam** até o valor (count-up com mola) sempre que mudam.
- **Preço ao vivo:** o valor que andou pisca na cor do sentido (verde sobe,
  vermelho desce) e uma seta ▲/▼ aparece ao lado por 1,6 s — a seta diz o
  sentido sem depender da cor. No patrimônio, só a seta: o número já conta.
- **Rosca (donut)** da carteira: os arcos se desenham (`pathLength`) ao entrar;
  no resultado do aporte, a rosca **transita** do antes para o depois.
- **Barras de classe:** a parte atual cresce da esquerda; a parte que o aporte
  acrescenta aparece em listras douradas animadas ("vai entrar"), até o marcador
  de meta.
- **Cartões do plano** entram em cascata (`STAGGER`, 30 ms), cada um com mola e um
  leve blur→nítido. O valor de cada um conta até o final.
- **Botão principal** magnético (desloca alguns pixels em direção ao cursor),
  brilho que atravessa a superfície no hover, compressão ao clicar.
- **Registrar aporte:** confete dourado (`canvas-confetti`) a partir do botão +
  check desenhado; as barras da carteira já aparecem atualizadas.
- **Abas:** indicador que desliza entre elas (`layoutId`); a tela que sai some
  em 90 ms e a nova entra com fade + 6 px de deslocamento em 200 ms
  (`screenTransition`).
- **Toggles do checklist:** pílulas grandes com botão de mola; a nota do ativo
  vira (flip) a cada resposta.
- **Esqueletos** com brilho deslizante enquanto carrega.
- **`prefers-reduced-motion`:** `MotionConfig reducedMotion="user"`; o fundo para
  de derivar, o prumo aparece parado e o confete não dispara.

## Telas

Celular primeiro. No celular, barra de abas embaixo; de 960 px para cima, um
trilho à esquerda — estreito (84 px, ícone com o nome embaixo) até 1599 px e
com o nome ao lado do ícone a partir de 1600 px. O conteúdo vai até 1680 px
nas telas redesenhadas (`<Page>`) e fica na coluna de 1100 px nas outras. A
grade, a densidade, a tabela compacta e quando usar ⓘ, recolher ou folha
estão em [layout.md](layout.md).

### Navegação

- A barra lista as telas de `APP_ROUTES` (`frontend/src/app/routes.tsx`): tela
  nova é **uma linha** nessa lista, e a barra (que reparte a largura entre
  quantas houver), o título da aba, o `aria-current` e o roteamento saem dela.
  Hoje: Aportar, Carteira, Renda, Desempenho, Análise, Ajustes.
- Ajustes agrupa **seções** (`SETTINGS_SECTIONS`,
  `frontend/src/features/settings/sections.ts`): Metas por classe, Critérios,
  Objetivos, Análise, Importar da B3, Impostos e Alertas.
  Seção nova também é uma linha. Com espaço (≥ 1000 px de conteúdo), a lista
  de seções é uma **coluna à esquerda**, fixa ao rolar, e a seção ocupa o
  resto; mais estreito (celular, iPad em pé), as seções viram **pílulas numa
  linha só** que rola de lado, com a seção aberta trazida à vista. Os links
  têm `aria-current="page"` na atual; cada seção tem endereço próprio
  (`#/ajustes/metas`, `#/ajustes/criterios`, `#/ajustes/objetivos`, `#/ajustes/analise`,
  `#/ajustes/importar-b3`, `#/ajustes/impostos`) e uma seção por vez na tela.
- Os links antigos `#/metas` e `#/criterios` seguem valendo: abrem a seção
  certa de Ajustes e trocam o endereço pelo novo, sem entrada nova no
  histórico (`LEGACY_ROUTES` em `lib/route.ts`). O mesmo vale para a seção que
  mudou de nome: `#/ajustes/saude` (a Saúde da carteira) abre `#/ajustes/analise`
  (`LEGACY_SECTIONS`).
- No canto (junto do tema; na barra do desktop) ficam o **olho** e o tema.

### 1. Aportar (abertura)

1. Pergunta grande: **"Quanto você vai aportar?"**
2. Campo de valor enorme, em reais, com máscara (digitar `600000` mostra
   `R$ 6.000,00`), foco automático.
   - **Caixa**, logo abaixo do campo, quando há saldo: "+ R$ 812 em caixa —
     **Incluir**". Incluir soma o saldo ao valor digitado; "Tirar" desfaz; digitar
     um valor menor que o caixa incluído desfaz a inclusão (o caixa nunca passa
     do aporte). Um recolher, "De onde vem esse caixa", diz quanto vem de
     proventos recebidos e de vendas, quanto os aportes já usaram e os movimentos
     mais novos, cada um com o sinal escrito. Um ⓘ na linha do caixa ("Como o
     caixa entra no aporte") diz desde que dia a conta vale e lembra que, para
     aportar dinheiro de fora, basta digitar o valor total: só o que se incluiu
     do caixa sai dele. Ao registrar, a folha avisa "R$ X deste aporte saem do
     caixa" e o saldo baixa. Com o aporte parcial (só parte do plano
     registrada) o caixa é gasto primeiro e nunca passa do registrado: `cash_used`
     = o menor entre o caixa incluído e a soma dos marcados.
3. Chips rápidos: R$ 500 · R$ 1.000 · R$ 2.000 · R$ 5.000 · último aporte.
4. Botão **Calcular** (Enter também calcula).
5. A **linha de cima** (docs/layout.md, 7.1): no desktop (a partir de 1000 px
   de conteúdo) o campo fica na coluna da esquerda, compacto (prumo e pergunta
   na mesma linha), e ao lado dele **A carteira** (rosca e barras por classe,
   um ⓘ explica o traço da meta). **Antes do cálculo** o painel mostra a
   carteira **de hoje** (sem "depois", sem alternância): o campo já nasce no
   lugar e no tamanho em que vai ficar, em vez de pular da página inteira para
   a coluna quando o plano chega, e a pessoa vê o ponto de partida enquanto
   decide quanto aportar. **Com o plano** o mesmo painel vira **A carteira
   depois do aporte** (Antes/Depois; o pedaço dourado é o que o aporte soma).
   Sem carteira para mostrar, o campo fica sozinho e centrado. Em uma coluna só
   (celular, iPad em pé) o painel desce para depois do que comprar: a ordem no
   DOM é a do desktop, e o `order` do CSS só troca a ordem visual.
6. Resultado, sem trocar de tela. A região "Resultado do aporte" reúne o que
   segue, abaixo do campo e da carteira:
   - Painel **"Compre 5 ativos"**, na largura inteira, com "com **R$ 5.999,97**
     · troco de …", a frase da renda com o selo "estimativa" e, no canto, o
     botão **"Registrar N comprados"** (no celular e no iPad em pé ele flutua
     no rodapé; desligado enquanto nada está marcado).
   - **Cartões** numa linha, até onde couberem (mínimo de 200 px), quebrando
     para a seguinte; dois por linha no celular: classe, ticker em destaque,
     nome, valor, "**12 cotas** × R$ 53,81" e a renda do cartão.
   - Abaixo, dois painéis lado a lado: **Avisos (N)** e **Seus objetivos**.
   - Avisos em linguagem simples ("Ações Internacionais está abaixo da meta,
     mas você não tem nenhum ativo lá — R$ 37.800 ficaram sem destino"), cada
     um com a ação que resolve. "XPTO3 está a R$ 32,50, acima do seu teto de
     R$ 30,00" abre a folha do ativo, onde o teto está; "XPTO3 chegou no seu
     limite de 15% da carteira" (e a classe bloqueada pelo limite) leva ao
     controle do limite.
   - Ativo **sem teto** de preço, com Bazin e/ou Graham: o cartão leva, abaixo
     da renda e separado por um filete, um bloco curto (`role="note"`): o rótulo
     "Sem teto" com um ⓘ e um chip por método, "✓ Bazin R$ 18,69" ou
     "✗ Graham R$ 77,77". ✓ = o preço de hoje está até o do método (igual, no
     centavo, conta, como no teto): o método **aprova** a compra a esse preço;
     ✗ = acima, **não aprova**. O veredito está no ícone e no nome acessível do
     chip ("Bazin R$ 18,69: não aprova"); a cor (verde/vermelho) só reforça. A
     explicação fica num **toggletip** (`components/Toggletip.tsx`): abre no
     toque (iPad), no foco e ao passar o mouse, fecha com Esc ou tocando fora, e
     entra numa região `role="status"`; o balão abre acima do bloco, porque o
     cartão corta o que passa da borda. O ⓘ diz que o teto é opcional, que são
     referências públicas e que a decisão de aportar é da pessoa. Só informa:
     não bloqueia, não preenche o teto e não tem botão de ação. Com o olho
     fechado, os R$ saem mascarados (chip, nome acessível e explicação) e o
     veredito continua. Ativo com teto, ou sem nenhuma das duas referências, não
     tem o bloco.
   - **"Comprei", por ativo.** A pessoa pode não comprar um ativo, ou comprar
     outra quantidade, então a compra é marcada cartão a cartão:
     - Cada cartão tem o botão **"Comprei"** (nome acessível "Comprei NVDA").
       Ele abre uma folha pequena "Comprei NVDA" com a **quantidade** e o
       **preço pago**, preenchidos com os do plano e editáveis, e o total. A
       unidade é a do ativo (`QuantityInput`/`MoneyInput`): inteiro em
       ação/FII/ETF, fração em cripto e título do Tesouro, valor aplicado em
       renda fixa por valor. Uma folha, e não um balão, porque o campo numérico
       sobe o teclado no celular e a folha prende o foco e fecha com Esc; o
       cartão corta o que passa da borda, e um balão não cabe nele.
     - Marcado, o cartão diz em texto **"✓ Comprado: 7 cotas × R$ 54,36"**
       (borda verde) e, quando a quantidade ou o preço diferem do plano, **"Plano:
       8 cotas × R$ 53,81"** (borda dourada); trocam o botão por **Editar** e
       **Desfazer** (com o ativo no nome acessível). O estado nunca é só cor.
       O foco vai para o botão que nasceu no lugar do usado.
     - Atalho **"Comprei todos como sugerido"** (marca todos com o plano); com
       algum marcado vira "Marcar os demais como sugerido" e só completa os que
       faltam, sem desfazer o que a pessoa ajustou. Uma linha diz "2 de 4
       marcados · 2 ficam de fora deste registro", sem julgamento.
     - **"Registrar N comprados"** abre a conferência "Registrar aporte": a lista
       só dos marcados, o total, "R$ X deste aporte saem do caixa" e quantos
       ficaram de fora. Registra **um** aporte (`POST /contributions`): `amount` =
       a soma dos marcados, `items` com a quantidade e o preço informados e
       `cash_used` pela regra do caixa acima. Depois, o mesmo confete, a mesma
       mensagem e o mesmo refresh de antes.
     - **Recalcular** (valor novo, chip, nota nova, ativo novo) limpa as marcas:
       elas valem para o plano em que foram feitas. Com o olho fechado, todo R$ e
       toda quantidade saem mascarados, e a folha "Comprei" não abre já com o
       foco no campo (para não revelar a quantidade sozinha).
7. Carteira vazia: o prumo balançando e um convite de um toque para montar a
   carteira (assistente de três passos: perfil → metas, primeiro ativo, pronto).

### 2. Carteira

- Layout denso (docs/layout.md § 7.3): `<Page>` com título e um botão
  **Adicionar ativo** no cabeçalho (some o botão flutuante), uma **faixa de
  quatro cartões** e, abaixo, a grade `main-aside` (2/3 + 1/3): à esquerda um
  painel por classe, à direita a rosca por classe, as roscas por setor e o
  histórico de aportes recolhido. Em 1440×900 a tela mede ~1,3 (antes 3,0).
- A faixa: **Patrimônio** (count-up; a linha do rótulo traz o **● Ao vivo**
  com o aviso "B3 com 15 min de atraso", "Conectando ao vivo…" ou a hora da
  última cotação, e o botão de atualizar), **Lucro total**, **Abaixo do ideal**
  e **Análise**. No celular o patrimônio e a análise ocupam a linha inteira.
- **Lucro total**: "+R$ 3.150" — o ganho de capital (realizado + não
  realizado) mais os proventos recebidos. Um ⓘ ("Como se chega no lucro
  total") abre as três parcelas. O sinal é escrito (`+` / `−`), nunca só cor, e
  anda com o preço ao vivo (o evento `prices` leva o bloco). Quando algum ativo
  está sem preço médio, sem cotação ou com uma venda de custo desconhecido, o
  rótulo vira "Lucro total (parcial)" e o ⓘ diz quem falta ("Parcial: falta o
  preço médio de HGLG11 …"): o número é só do que se sabe, nada é estimado.
  Venda de ativo que já saiu da carteira tem aviso próprio no mesmo ⓘ. Com o
  olho fechado, o ⓘ também sai mascarado.
- Um painel por classe, na ordem fixa: numa linha, o nome, a barra com o
  marcador de prumo e "% atual · meta %"; embaixo, **uma linha por ativo**
  (botão que abre a folha): ícone, ticker e nome, quantidade · preço · PM, valor,
  rentabilidade, fatia da classe × ideal (com a barra fina) e a nota. A linha
  inteira é um botão (e não uma `<table>`) porque o teste de privacidade
  (`components/privacy.test.tsx`, congelado) lê quantidade e "da classe" no
  próprio botão da linha. Classe sem ativo diz "Tem meta e nenhum ativo." (o
  resto no ⓘ) com o botão Adicionar; classes sem meta e sem ativo ficam numa
  linha de chips.
- A rosca por classe tem **legenda escrita** (nome e %). Cada classe com setor
  conhecido ganha o seu painel de rosca por setor (por tipo de fundo nos FIIs).
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
  Na faixa do topo, o cartão **Abaixo do ideal** ("2 ativos", ou "Nenhum
  ativo"; o ⓘ explica o ideal); ele não aparece sem nenhum ativo que receba.
  Tudo acompanha o preço ao vivo, porque o evento `prices` leva o ideal.
- A **rentabilidade** é a própria coluna: `+9,2%` / `−3,0%`, em verde ou
  vermelho, com o **sinal escrito** (cor nunca sozinha; a seta ▲/▼ continua
  sendo só do preço que acabou de andar, para as duas coisas não se
  confundirem). O **preço médio** entra na coluna de quantidade ("10 cotas ·
  R$ 142,00 · PM R$ 130,00"). A linha responde à largura do painel (≥ 640 px:
  uma linha em colunas; abaixo, três linhas: nome · valor · nota, quantidade ·
  rentabilidade, atual × ideal). Sem custo conhecido, nem PM nem rentabilidade
  aparecem — nada de "0%" que não é verdade. A rentabilidade anda com o preço
  ao vivo.
- O botão **Adicionar ativo** abre uma folha (sheet) com **um campo só**: digita o
  ticker, aparece o autocompletar (`/symbols`), escolhe, a classe já vem
  sugerida em chips (troca com um toque), informa a quantidade, pronto. Para
  renda fixa: nome + valor, ou "Tesouro Direto" (busca pelo nome, quantidade
  fracionária, preço automático).
- Tocar num ativo abre a **folha larga** (`Sheet size="wide"`, ~1,0–1,2 do
  tamanho da folha; antes 1,9–2,4), em duas colunas: à esquerda o resumo
  ("Hoje" e a nota ao vivo), a quantidade com stepper, as chaves **Usar preço
  manual** e **Teto de preço** lado a lado, o imposto e o checklist em linhas
  (pergunta e sim/não na mesma linha) ou o controle de nota manual (0–10) para
  cripto e renda fixa; à direita os **Lançamentos** e os **Fundamentos**. As
  dicas (quantidade, preço manual, teto, nota na mão, fonte dos fundamentos,
  o que são os lançamentos) são ⓘ; a do imposto fica à vista porque muda com a
  escolha. **Salvar** e **Remover da carteira** ficam numa barra que acompanha a
  rolagem. A folha do checklist aberta do plano (`RateAssetSheet`) é a mesma.
  Abaixo de 640 px de largura a folha volta a uma coluna.
- Teto de preço: uma chave "Teto de preço" com a explicação curta no ⓘ ("Seu
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
- **Fundamentos**, dentro da folha, depois de Lançamentos, só em Ações
  Nacionais e FIIs. Mesmo desenho do resumo de Lançamentos (rótulo pequeno em
  cima, número tabular embaixo, fundo afundado), duas colunas no celular e três
  quando a coluna da folha comporta:
  - companhia: "Últimos 12 meses, até 30/09/2026" (ou "Exercício encerrado em
    …"), receita, lucro, ROE, margem líquida, dívida líquida sobre EBITDA e
    sobre o PL, LPA e VPA; embaixo, anos com lucro e com caixa operacional
    positivo ("4 de 5") e o crescimento de receita e lucro em 5 anos ("+10,0%
    a.a.", sinal escrito). Banco e seguradora não mostram as linhas que não se
    aplicam e dizem por quê numa dica. Recuperação judicial (ou outra situação
    do emissor que não a operacional) é um aviso dourado com ícone e texto;
  - FII: valor patrimonial por cota, P/VP (com o preço ao vivo da carteira),
    vacância física e patrimônio líquido;
  - número que falta é "—", com o motivo no `title` e em texto `sr-only`
    ("base zero ou negativa", "não se aplica a banco ou seguradora"), nunca zero;
  - a fonte e as datas no ⓘ do título ("De onde vêm os fundamentos"): "Fonte:
    CVM, ITR de 30/09/2026 · conferido em 26/11/2026";
  - ainda sem dado: "Buscando os dados na CVM"; sem fundamentos (ETF em Ações
    Nacionais, fundo fora do informe), a frase do motivo;
  - com o olho fechado, todo R$ (receita, lucro, LPA, VPA, valor patrimonial,
    patrimônio) sai como `R$ ••••`; percentuais, múltiplos e o P/VP ficam.
- A quantidade editada na própria folha vira um "ajuste" em Lançamentos (a
  dica sob o campo diz isso); compra e venda vão em Lançamentos, com data e
  preço.
- No cadastro, com quantidade, um campo opcional de **preço médio** (renda
  fixa: "quanto você aplicou"). Sem ele, o ativo entra e a tela pede depois.

### 3. Ajustes › Metas por classe

- Perfis prontos (Conservador, Moderado, Arrojado) numa fileira de botões
  segmentados no cabeçalho, que preenchem as metas; a descrição de cada perfil
  fica num ⓘ ao lado ("O que cada perfil pronto faz") e a frase "os aportes vão
  sempre para o que está mais longe da meta" no ⓘ do título.
- Um controle deslizante por classe, com a cor da classe, em duas colunas
  dentro do painel "Metas por classe" (uma coluna no celular).
- À direita (embaixo, no celular), "Resumo das metas": o anel de soma e a rosca
  lado a lado; o anel mostra o total, fica verde e "estala" (pulso) ao chegar em
  100%. Botão **Salvar** só habilita em 100%; botão **Equilibrar** distribui o
  que falta/sobra proporcionalmente.
- Embaixo do resumo, o painel **Limite por ativo**, com o próprio **Salvar limite** (não
  depende das metas fecharem 100%): uma chave "Limitar a concentração" que diz
  o estado salvo ("Desligado: cada ativo recebe pela nota, sem teto." / "Ligado
  em 15% da carteira.") e, ligada, um controle deslizante de 1% a 100% (começa
  em 20%). O ⓘ do título explica que o que passaria vai para os outros ativos da
  classe e que o motor nunca vende. Os avisos do plano sobre o limite levam
  para cá (`#/ajustes/metas`).

### 4. Ajustes › Critérios

- Duas listas (Ações e Imobiliário) lado a lado, num painel cada, com o número
  de perguntas no canto: perguntas editáveis no lugar, adicionar, remover,
  restaurar padrão.
- A regra da nota ("cada sim soma 1, cada não tira 1…") fica no ⓘ do título.

### 5. Análise (tela de topo) e suas regras (seção de Ajustes)

A Análise (`#/analise`) fica na barra principal, entre Desempenho e Ajustes. Ela
evoluiu a antiga "Saúde da carteira": as sete regras de antes entram nas
dimensões certas e as regras são da pessoa, que as configura em **Ajustes,
Análise** (`#/ajustes/analise`). O endereço antigo `#/ajustes/saude` continua
abrindo a seção nova (`LEGACY_SECTIONS` em `lib/route.ts`). Pontuação simples e
à mostra, nunca uma nota de marca: o percentual das regras ligadas, com dado,
que estão ok, e o número exato ao lado.

Na tela (`AnalysisScreen`, `<Page>`), de cima para baixo (o desenho e as
medidas estão em [layout.md](layout.md) § 7.5):

- **Cabeçalho** em uma linha: o título, o ⓘ com a explicação da tela (o que é
  cada dimensão e que regra sem dado não é falha) e o link **Ajustar as
  regras** à direita.
- **Primeira dobra.** À esquerda, o **veredito** (`role="status"`): o prumo
  parado e "Nenhum ponto a ver" quando está tudo dentro dos limites; "N pontos a
  ver" com quantas regras passaram; ou "Ainda sem o que medir". Nunca "tudo
  certo" quando não há o que medir. Logo abaixo, no mesmo cartão, o **perfil de
  risco** compacto: duas barras finas, **Agora** e **Suas metas**, renda fixa de
  um lado (**listrada**) e renda variável do outro (lisa), com o rótulo
  (Conservador, Moderado, Arrojado) e os dois percentuais **escritos**; a cor e
  a listra só ajudam. A barra é `role="img"` com a descrição completa. Embaixo,
  a diferença em pontos percentuais; os limiares do Prumo (30% e 70% de renda
  variável) ficam no ⓘ. Sem ativos ou sem metas, diz o que falta.
  À direita, as **quatro pontuações** (`DimensionScores`: Diversificação,
  Dividendos, Rentabilidade, Qualidade dos ativos), em 2 × 2 na tela larga e
  lado a lado no iPad em pé: a **pontuação em número grande** ("62,5%"), o selo
  "N a ver" (ou "Tudo ok"), uma barra `role="meter"` e o número exato ("5 de 8
  regras ok · 2 regras sem dado, fora da conta"). O cartão inteiro é um botão
  ("Ver as regras de Dividendos") que leva até a tabela da dimensão e põe o
  foco nela.
- **Uma tabela por dimensão** (`DimensionCard` + `RuleTable`, sobre a
  `DataTable`). O cabeçalho repete a pontuação ("3 de 9 regras ok 33,3%") e o
  ⓘ diz o que a dimensão mede. À vista ficam **só os pontos a ver**, uma linha
  por regra: a frase com o limite da pessoa ("Pelo menos 10% da carteira no
  exterior"), o **estado** (`StatusPill`: texto e ícone; Ponto a ver / Ok / Sem
  dado / Desligada), o **diagnóstico** em uma linha (a conta: "0% da carteira
  está no exterior."), **Agora** (com a barra fina do limite nas regras de teto
  em percentual), o **Limite** com o sentido escrito ("máx. 15%", "mín. 10%" ou
  o índice, "CDI 13,20%") e **O que fazer**: o link para a tela certa à vista e
  o texto inteiro — aportar (valor em reais) ou olhar uma tela do Prumo, nunca
  vender nem citar ativo de fora — no ⓘ da linha. As sete regras originais
  listam os **achados** no diagnóstico, um por linha, com o ícone dos ativos
  citados ("Tesouro IPCA+ 2035 é 70,76% da carteira", "Ações Nacionais tem 1
  ativo"); o "o que fazer" de cada achado fica no mesmo ⓘ. As regras **ok, sem
  dado e desligadas** ficam num recolher no fim ("3 regras ok · 1 sem dado"),
  na mesma tabela; "Sem dado" diz o motivo e **não** é aviso; "Desligada" diz
  que não entra na pontuação. Sem nenhum ponto a ver, a dimensão diz isso.
  No iPad em pé cada regra vira duas linhas (frase, estado e números em cima;
  diagnóstico e o que fazer embaixo) e, no celular, um bloco com a frase em
  cima.
- **Qualidade dos ativos**: uma dimensão como as outras. Cada regra diz
  "Entraram na conta 1 de 2 ativos. Abaixo do critério: FRAC3." — quem não tem
  o dado fica fora e aparece como "Sem dado", com o motivo (banco sem dívida,
  fundo de papel sem vacância). O "o que fazer" manda olhar a folha do ativo, em
  Fundamentos, e diz que o Prumo não manda vender.
- **Pontos de atenção dos ativos** e **Destaques de 12 meses**, lado a lado.
  Pontos de atenção (`AttentionCard`): uma lista por ativo, com o ícone dele e
  frases curtas e factuais ("ROE de 4,12%, abaixo de 10%", "Provento de 12
  meses caiu 22,50% em relação aos 12 meses anteriores"), cada uma com o ícone
  de alerta (o texto diz tudo). Sem ponto, "Nenhum ponto de atenção nos N ativos
  com fundamentos"; sem nenhum ativo avaliado, "Ainda sem dados de
  fundamentos", nunca "tudo certo". Rodapé de uma linha: quantos foram
  avaliados, quantos esperam a CVM e "não são recomendação". Destaques: duas
  medidas (preço em 12 meses e rentabilidade desde a compra), cada uma com
  "Mais subiram" e "Mais caíram" (até 3), o ícone, o nome e a variação **com
  sinal** (+18,40%, −12,80%); como cada medida é tirada fica no ⓘ.
- **Referências de preço** (`ReferencesCard`): o aviso fixo (`role="note"`)
  "Referência pública, não é recomendação" em uma linha; as três fórmulas (com
  o rendimento-alvo de hoje) no ⓘ do título e "Mudar o rendimento-alvo" à
  direita. Uma **tabela compacta**: Ativo, **Bazin** (a média de dividendos
  usada no ⓘ da célula), **Graham** (só com lucro e patrimônio por ação
  positivos), **Pelas duas** (o menor dos dois, só quando há os dois), **Seu
  teto**, **Hoje** e, na linha, o botão "Usar R$ 9,50 como meu teto" (com o
  valor pelas duas; a pessoa não escolhe fórmula), que abre uma folha de
  **confirmação** ("Definir R$ 9,50 como teto de preço de ITSA4?") antes do
  `PATCH`. Célula sem valor mostra "—" e diz o motivo ao leitor de tela. Nada de
  "oportunidade", "compre", "preço justo" ou "upside".
- **De onde vêm os dados**, recolhido no fim: a fonte de setor de cada classe,
  de quando é, se a última busca falhou (a falha aparece já na frase fechada:
  "1 fonte falhou"), quantos ativos ficaram sem setor e por que os EUA não têm
  setor.

Em **Ajustes, Análise** (`AnalysisSettings`, `<Page>`), as dimensões em duas
colunas (Diversificação, Dividendos e o rendimento-alvo do Bazin à esquerda;
Rentabilidade e Qualidade à direita; uma coluna no celular), um painel por
dimensão com uma linha por regra: a frase corrida com o campo do limite no
meio ("Nenhum ativo acima de [15] % da carteira"), só nas regras com limite
editável, o porquê no ⓘ colado na frase e o interruptor à direita. "Voltar
para 60%" aparece quando o limite não é o de fábrica. **Salvar regras** só
habilita com mudança válida; com mudança por salvar, a barra de salvar
acompanha a rolagem no desktop. O erro de um limite aparece na própria linha
(`role="alert"`, `aria-invalid`). A Qualidade tem as suas oito regras (a de
recuperação judicial só liga e desliga). Um painel à parte guarda o
**rendimento-alvo do Bazin** (% ao ano).

Na **Carteira**:

- Na faixa do topo, o cartão **Análise** ("3 pontos a ver", "Nenhum ponto a
  ver" ou "Ainda sem o que medir") com o link "Ver análise" para `#/analise`; os
  dois primeiros pontos, com o número de agora e "e mais N", moram no ⓘ. Junto
  com o "Abaixo do ideal", são os dois resumos do topo: o do ideal diz onde
  aportar, o da análise diz o que passou do limite. Some se a análise não
  carrega, para nunca atrapalhar a carteira.
- No painel lateral, uma **rosca por setor** de **Ações Nacionais** e de
  **Fundos Imobiliários** (por tipo de fundo nos FIIs) com a lista ao lado: nome e fatia sempre
  escritos; a cor só ajuda a achar. Sete setores têm cor; o resto vira "Outros
  setores", e o que a fonte não conhece vira "Sem setor" (cinza). Se a fonte
  nunca respondeu, uma linha diz isso no lugar da rosca.

Com o **olho fechado**, todo valor em reais da Análise vira `R$ ••••` (por
`useMoney()`, também para o leitor de tela e para a folha de confirmação): o
aporte sugerido (no ⓘ do "o que fazer"), o que o pagador pagou, a renda de 12
meses e a base do yield (o valor das ações e FIIs), o Bazin e a média de
dividendos do ⓘ, o Graham, o "pelas duas", o teto, o preço de hoje e o botão de
teto (cujo nome acessível não leva valor). Percentuais, contagens,
limites, o perfil e os destaques são percentuais e ficam à vista. O título do
Tesouro Direto (renda fixa) não tem setor e aparece nos achados pelo nome
("Tesouro IPCA+ 2035"), não pela chave `TD-…`.

A paleta dos setores (`--sector-1` a `--sector-7`, `--sector-other`,
`--sector-none`) tem tom próprio nos dois temas e fica em `styles/health.css`;
todos passam de 3:1 sobre o fundo (elementos gráficos), e o texto de cada fatia
está na lista. O resto da Análise mora em `styles/analysis.css`.

### 6. Desempenho

Na barra principal, depois de Carteira.

Responde "estou ganhando do CDI?". Método e limites em
[`desempenho.md`](desempenho.md).

- **Layout denso** (`<Page>`, `docs/layout.md` § 7.4): o título é uma linha com
  o ⓘ da explicação do método, e os chips de período (Mês · No ano · 12 meses ·
  Desde o início) ficam à direita dele, nas ações do cabeçalho.
- **Faixa de seis números** (`StatStrip`, uma linha no desktop, duas colunas no
  celular): **Últimos 12 meses** e **Desde o início**, com a rentabilidade
  assinada (`+1,23%`) e, por baixo, "**1,13% acima do CDI** (118% do CDI)"; as
  datas e "o CDI rendeu +0,10%" ficam no ⓘ do número, e "a série ainda não tem
  12 meses" fica escrito no detalhe. Depois **Patrimônio**, **Aplicado**,
  **Ganho** (o ganho é o do preço: no ⓘ) e **Proventos**.
- **Os dois gráficos lado a lado** (`Grid cols={2}`, uma coluna abaixo de 640
  px de conteúdo; 196 px de altura, 270 px em monitor largo): **Patrimônio no
  tempo**, com a área dourada do patrimônio e a linha tracejada neutra do
  aplicado, e **Rentabilidade contra os índices**. O selo "Hoje é parcial"
  continua escrito no título do primeiro, e o texto "o ponto de hoje usa o
  preço do momento" vai no ⓘ do painel.
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
  aparece como "sem dado", nunca como zero; o ⓘ do painel diz até quando cada
  índice foi publicado e quem ainda está sem dado.
- **Mês a mês:** painel de largura inteira, abaixo dos gráficos, com os anos
  (o mais novo em cima), os doze meses, o ano e o CDI do ano; no celular rola de
  lado dentro do painel. É também a alternativa ao gráfico para leitor de tela (o
  gráfico é `role="img"` com um resumo em texto).
- **Gráficos:** SVG próprio, linha de 2px que se desenha ao entrar
  (`pathLength` com mola), grade discreta, zero marcado na rentabilidade. Cores
  das linhas, validadas para daltonismo e contraste ≥ 3:1 nos dois temas:
  carteira âmbar (o dourado do dinheiro), CDI azul, IPCA laranja, Ibovespa
  violeta — e cada índice com o próprio tracejado, para a cor não ser a única
  pista. Texto nunca usa a cor da série. Alta e baixa levam sinal (+/−) além
  do verde/vermelho.
- **Olho fechado:** todo valor em R$ (os números da faixa, o eixo e a
  régua do gráfico de patrimônio e o resumo para o leitor de tela) sai por
  `useMoney()` como `R$ ••••`, e o ganho perde o sinal e a cor. As
  rentabilidades são percentuais e ficam à vista, como na Carteira.
- **Estado vazio honesto:** sem lançamento, "Ainda não há série" com o caminho
  para a Carteira e para a importação da B3; com uma foto só, "A série começa em DD/MM/AAAA" e nenhum
  número inventado.

### 7. Ajustes › Importar da B3

- Duas colunas: "Suas planilhas" e, ao lado, o passo a passo em quatro itens
  de como baixar as planilhas na Área do Investidor (sem imagem de terceiros);
  o que a planilha não traz (corretagem), o limite do histórico e o prazo dos
  arquivos ficam num recolher "Limites da B3 e o que a planilha não traz". A
  frase de cima diz o que importa: nada é gravado antes de Importar.
- Seletor de um ou mais `.xlsx`; cada arquivo da lista mostra o que foi lido
  (tipo, linhas, quantas entram, quantas não foram entendidas) ou, se não abriu,
  o motivo em português.
- Prévia: duas faixas de números (o que foi lido; e, contra a carteira,
  quantas linhas **entram**, **já existem**, **conflitam** e **ficam de
  fora**). Tabela compacta agrupada por ativo (rola de lado no celular): classe (num ativo novo, um seletor com a sugestão; no que
  já está na carteira, a classe dele), a situação (selo com texto: "entra na carteira",
  "já está na carteira", "nada novo", "deixado de fora", mais "conflito" e
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

**Layout denso** (`<Page>`; meta ≤ 1,2 tela em 1440x900, ver `docs/layout.md`
§ 7.4): o título "Quanto sua carteira paga?" é uma linha com o ⓘ da explicação
e o botão **Lançar provento** à direita.

1. **Meta de renda mensal** (painel à esquerda do topo). Número grande (média
   mensal dos 12 meses recebidos), barra de progresso até a meta
   (`role="progressbar"` com texto para o leitor de tela: "R$ 135 de R$ 800 por
   mês, 17%"), "Editar meta" no cabeçalho do painel e a data em que foi
   atingida, quando foi. A explicação da base ("Média do que você recebeu…") vai
   no ⓘ. Sem nada recebido ainda, a barra usa a **estimativa** e fica listrada,
   com a etiqueta "estimativa" ao lado do número: o desenho muda, não só a cor.
2. **Quatro números** (`StatStrip` à direita da meta, em 2x2 na altura do painel
   da meta): recebido em 12 meses, média por mês, a receber e o **yield on
   cost** da carteira. O yield é a renda dos 12 meses sobre o que foi pago; a
   conta ("Renda dos últimos 12 meses (recebida) ÷ o que você pagou. Hoje: R$ X
   sobre R$ Y") está no ⓘ. Sem recebido, a renda é a estimada, e o número diz
   "(estimado)", e o ⓘ, "estimada, não é extrato". Ativo sem preço médio sai da
   conta: o rótulo vira "Yield on cost (parcial)" e o ⓘ diz quem ficou de fora.
   Sem nenhum custo conhecido, no lugar do número vem "informe o preço médio".
3. **Mês a mês** (linha do meio, 2/3): barras em SVG próprio com Motion,
   desenhadas na largura real do painel (a barra não passa de 48 px). O recebido
   é cheio e embaixo; o a receber é **listrado** e em cima; a meta é uma linha
   tracejada com o valor; o ponteiro acende o mês. A legenda dá nome a cada um e
   uma tabela escondida (`sr-only`) traz os 12 meses para o leitor de tela. À
   direita (1/3), **Próximos pagamentos** (data, ativo, tipo e líquido; o
   provento que o Prumo calculou do anúncio da B3 leva o selo **"automático
   (B3)"**, borda pontilhada e texto) com a **altura do mês a mês**: a lista
   rola por dentro, então pagamento demais não alonga a página. O painel
   **Eventos anunciados pela B3** (só com os automáticos ligados e alguma
   pendência: uma frase por bonificação, desdobramento ou grupamento e as cotas
   da data com) fica embaixo dela.
4. **Quem pagou** (linha de baixo, 2/3, ao lado da estimativa): para **explorar**.
   A rosca e uma tabela (Pagador, Recebido, % da renda, Yield on cost: "6,2%" ou
   "informe o preço médio"); os seis maiores um a um e o resto em "Outros (N)",
   sem yield.
   - **Entrar em "Outros"** (decisão: a rosca *entra* no grupo, em vez de expandir
     por baixo ou abrir folha). Clicar em "Outros" na tabela, ou na fatia (ou
     Enter nela), troca a rosca e a tabela pelas dos N pagadores de dentro, com
     o mesmo valor, parte e yield on cost de cada um, e um "← Voltar a todos os
     pagadores" no cabeçalho do painel; o foco vai para o "Voltar" e, ao voltar,
     para "Outros". Razão: a lista de 19 ganha a mesma área da de 7, sem
     empurrar a página nem esticar o painel ao lado; folha esconderia a rosca,
     e expandir por baixo faria a página pular. A tabela tem altura fixa
     (20 rem, 34 rem no celular) e rola por dentro; uma linha cortada embaixo
     mostra que há mais.
   - **Filtrar por pagador.** Clicar numa linha (ou na fatia) a destaca (fundo
     dourado, filete e "✓" no nome, `aria-pressed`) e filtra **Próximos
     pagamentos** e **Proventos lançados** só por ele; um selo "Filtro: BBDC4 ✕"
     no cabeçalho dos painéis filtrados limpa; clicar de novo no pagador também.
     Sem pagamento dele, a lista diz isso. O filtro de quem está dentro de
     "Outros" some ao voltar (a linha dele não está mais à vista).
   - **A rosca responde** ao mouse, ao foco e ao toque: o meio mostra o nome, o
     valor e a parte de quem está aceso (senão, "12 meses" e o total), o arco
     engorda e os outros escurecem; a linha da tabela acende junto, e passar
     numa fatia traz a linha dela para a janela da tabela. Cada arco é um
     botão com nome ("BBDC4: R$ 529,58, 9,6% da renda"); o teclado entra por
     uma parada só (as setas, Home e End andam entre as fatias; Enter ou
     Espaço escolhem). Com o olho fechado, o meio, o nome acessível e a tabela
     saem mascarados.
   - No celular a rosca sai (a tabela já diz tudo, em duas linhas por pagador).
5. **Estimativa da carteira de hoje** (linha de baixo, 1/3): "~R$ 410/mês" com a
   etiqueta "estimativa", os **cinco** papéis que mais rendem numa tabela (Ativo,
   Por cota, Por mês; "por cota" é o que ela pagou em 12 meses) e os papéis sem
   histórico. Com mais de cinco, **"Ver todos os N ativos"** abre uma folha com
   a lista inteira, do que mais rende ao que menos, com as cotas. Foi folha, e
   não expandir no painel, para a lista de 30 linhas não esticar o painel nem
   deixar um vazio no "Quem pagou" ao lado. O aviso de que é o que a cota pagou,
   sem separar JCP e sem ser extrato, abre no ⓘ.
6. **Ano por mês:** tabela real (`<table>`), uma linha por ano, na largura
   inteira.
7. **Proventos lançados:** painel recolhido (`FoldPanel`, `<details>`) com a
   contagem; aberto, a lista com apagar (confirmação na própria linha). O
   automático tem o mesmo selo, e a confirmação diz que ele não volta na
   próxima busca.
8. **Lançar provento:** folha com ativo, tipo (chips), data de pagamento, valor
   por cota, quantidade e data ex opcional. **Em branco, a quantidade vale a do
   livro na data com** (não a de hoje): o campo mostra o número que o servidor
   vai usar ("Em branco vale o que o livro tinha na data com (09/11/2026)") e ele
   muda quando a data ex muda. Sem cotas naquela data, a folha avisa e só deixa
   lançar com a quantidade informada. A
   prévia mostra bruto, IR retido e líquido (JCP perde 15% até 2025 e 17,5%
   desde 2026, pela data de pagamento) e fala com
   `aria-live`. Renda fixa por valor pede só o valor recebido. A folha não
   mudou no redesenho (cabe em uma tela).

Na tela **Aportar**:

- Abaixo do campo de valor, quando há caixa: "+ R$ 812 em caixa —
  **Incluir**" (proventos recebidos e vendas, menos o que aportes anteriores já
  usaram; ver o item 1 de "Aportar"). Incluir soma ao valor; "Tirar" desfaz. Só
  mexe no valor de entrada.
- No resultado, abaixo da manchete, uma frase com o selo "estimativa": "Este aporte soma ~R$ 35/mês
  de renda: sua renda vai de R$ 410 para R$ 445 por mês". Cada cartão mostra a
  renda mensal que ele soma. O plano não espera o provedor de dividendos: sem
  histórico em cache ainda, a frase vira "Estimativa de renda indisponível por
  enquanto".
- Com o olho fechado (ver "Ocultar valores"), tudo isso sai mascarado — meta,
  barras, tabelas, rosca, prévia do lançamento e a quantidade em branco (o que o
  livro tinha na data com), a renda do plano, o caixa (saldo, detalhe e
  movimentos), o lucro total, os valores do yield on cost e as cotas da data com das pendências da B3 —, inclusive o que só o leitor de tela lê (`aria-label`,
  `aria-valuetext`, tabelas `sr-only`). Só a proporção (percentual) continua — o yield on cost é percentual e fica; o
  motivo de um yield ausente ("informe o preço médio") e o "parcial" do lucro
  total também, porque são texto, não valor.

### 9. Ajustes › Objetivos

Seção de Ajustes (`#/ajustes/objetivos`, uma linha em `SETTINGS_SECTIONS`):
metas com prazo, de patrimônio total ou de valor numa classe. A meta de renda
mensal continua na tela Renda (uma linha sob o título aponta para lá; o resto
da explicação fica no ⓘ do título).

- **Em andamento**: um cartão por objetivo, em grade (três por linha no
  desktop, um no celular), com o nome ("Patrimônio total",
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

Na tela **Aportar**, abaixo do resultado do plano, um painel "Seus objetivos"
(só aparece com objetivo ativo; se a consulta falha, some e o plano segue):

- "Com este aporte, você chega a **25,6%** da meta de R$ 1.000.000 (hoje,
  25,0%)." Se o aporte fecha a meta: "você completa a meta de R$ …".
- "Com aportes assim, a meta chega em **fev/2031**." (com aporte informado:
  "Com aportes de R$ 1.500 por mês, …"). A data conta a partir do valor que a
  carteira passa a ter com o aporte.
- Uma barra fina com o que a meta já tem (cinza) e o que o aporte soma
  (dourado), só como reforço do percentual escrito.
- As duas premissas ficam no ⓘ de cada objetivo ("Premissas da conta: …"), e o
  link "Trocar premissas" leva a Ajustes › Objetivos, onde se troca cada uma.
- Com o olho fechado, a meta e o aporte saem como `R$ ••••`, inclusive nos
  `aria-valuetext` e nos campos da folha; ficam o percentual, o mês estimado e
  os nomes.

### 10. Ajustes › Impostos

Seção de Ajustes (`#/ajustes/impostos`, uma linha em `SETTINGS_SECTIONS`).
Responde "devo DARF este mês?" e "o que vai na declaração?" sem esconder que é
estimativa. A regra e a ligação com o livro estão em `docs/ir.md`.

**Layout denso** (`<Page>`; 1,8 tela em 1440x900, antes 4,3): o título "Imposto de
renda" é uma linha com o ⓘ da explicação, e os chips do ano-calendário ficam à
direita dele.

1. **Aviso de responsabilidade**, sempre visível no topo (`role="note"`), em
   faixa dourada e **em uma linha**: estimativa, não substitui o contador, a
   responsabilidade é de quem declara. "Confira cada DARF… multa e juros" vai
   no ⓘ da faixa.
2. **Ano-calendário**: chips com os anos que têm regra (hoje 2025 e 2026). Abre
   no ano de hoje; sem regra para ele, no último com regra. Ano sem regra no
   servidor vira um cartão de erro com o motivo ("ainda não a de 2027"), nunca
   a conta de outro ano.
3. **Meses do ano** (`DataTable`, esquerda, 2/3): uma linha por mês: vendas de
   ações, imposto do mês, DARF (código e valor), vencimento e situação. A
   situação é um `StatusPill` escrito, "A pagar" (dourado, com ícone) ou "Pago"
   (verde, com ícone), com o botão "Marcar como pago"/"Desmarcar" na linha (o
   nome acessível diz o código e o mês). DARF abaixo de dez reais aparece como
   "abaixo do mínimo, soma no mês seguinte". Se o valor mudou depois de
   marcado, a linha avisa. As regras do DARF (6015, 4600, vencimento, mínimo de
   dez reais) estão no ⓘ do painel. Embaixo, o prejuízo a compensar em 31/12.
   No celular a tabela empilha, e o mês sem nada fica só com o nome.
4. **Isentômetro** do mês de hoje (dezembro, num ano passado), na coluna da
   direita: o número grande das vendas de ações, "de R$ 20.000,00 isentos", a
   barra (a mesma `LimitBar` dos avisos, com o fio de prumo no limite e o
   percentual em texto) e a frase: dentro do limite, quanto ainda cabe; passou,
   "o lucro das ações neste mês paga 15%", em vermelho e escrito (a cor só
   reforça). O lembrete de que ETF, BDR e FII não têm essa isenção vai no ⓘ;
   a cripto contra o limite dela continua escrita quando houve venda.
   Abaixo, o **Saldo de antes do Prumo**: o que foi informado (ou "a conta começa
   do zero em 1º de janeiro de 2025") e "Informar saldo"/"Editar saldo", que
   abre o ano (chips) e os seis campos de dinheiro em duas colunas, e os
   recolheres **O que o Prumo não calculou** e **Nunca entra na conta**.
5. **Declaração do ano** (painel de largura inteira): **Bens e Direitos** numa
   `DataTable` (ticker com o ícone, ficha com grupo, código e rótulo com acento,
   quantidade, situação em 31/12 pelo custo, preço médio) com **Copiar** por
   linha; abaixo, em duas colunas, os rendimentos isentos (09, 99, 20, 05, com o
   valor por pagador) e a tributação exclusiva (JCP, líquido, bruto e retido),
   também em `DataTable`; e a renda variável do ano (DARF do ano e o prejuízo
   que passa para o ano seguinte).
6. **Como cada mês foi apurado** (recolhido, com a contagem dos meses que têm
   movimento): por mês, um `<details>` com as vendas, a apuração por grupo
   (resultado, prejuízo antes, compensado, base, alíquota, imposto, prejuízo
   que fica), a cripto e as dúvidas daquele mês. **O que o Prumo não calculou**
   (recolhido, com a contagem): cada item do livro ou dos proventos que ficou
   fora, com ticker, data, valor e o motivo em uma frase. **Nunca entra na
   conta** (recolhido): a lista fixa do que nunca entra (IR retido da nota,
   corretora, opções, exterior…).
7. **Pontos a confirmar com contador**: as dúvidas de `docs/ir.md` § 5 são um
   **ⓘ azul** (`DoubtTip`) ao lado do número ou do bloco que elas mexem (o
   isentômetro, os títulos da declaração); o nome do botão diz do que se trata
   ("Ponto a confirmar com contador: JCP levado pelo valor líquido") e o texto
   abre no toque, no foco ou com o mouse. Na apuração de um mês, o ⓘ leva o
   nome da dúvida escrito ao lado.

Na **folha do ativo** (Carteira), abaixo do teto: "No imposto de renda", um
`<select>` com os sete instrumentos, o padrão marcado "(padrão)" e a frase do
que a escolha muda ("15% sobre o lucro, sem a isenção das ações…") e qual é o
padrão. Renda fixa por valor não tem a escolha.

Com o olho fechado, todo R$ e toda quantidade da seção (isentômetro, tabela,
apuração, declaração, não calculado, saldo) sai mascarado, inclusive no nome
acessível dos botões e da barra; percentuais, datas, códigos e tickers ficam. O
"Copiar" leva o valor de verdade para a área de transferência: é o que a pessoa
pediu, e a tela continua mascarada.

### 11. Ajustes › Alertas

Seção `#/ajustes/alertas` (`<Page>`). Responde "como o Prumo me avisa fora do
app?" sem virar painel: o cabeçalho em uma linha (a explicação no ⓘ) e quatro
painéis em **duas colunas** (uma no celular) — à esquerda Telegram e "O que
avisar", à direita "Quando e com quê" e o exemplo do resumo. O funcionamento e
a configuração do bot estão em [alertas.md](alertas.md).

1. **Telegram.** Quatro estados, cada um com texto (nenhum só por cor):
   - **Desligado no servidor** (sem token do bot): uma frase "os alertas estão
     desligados neste servidor", o nome da variável e a indicação do
     `docs/alertas.md`. Não é erro (`role="status"`, não `alert`) e a tela não
     mostra o resto.
   - **Sem vínculo:** "Vincular o Telegram".
   - **Vinculando:** os passos numerados com o link `t.me/<bot>?start=<código>`,
     o comando `/start <código>` em destaque (selecionável de um toque), a
     validade ("uma vez só, até 15:10") e "Cancelar". A tela confere o estado a
     cada 3 s e, quando o chat liga, volta ao estado "vinculado" com um aviso
     "Telegram vinculado.".
   - **Vinculado:** "Vinculado em 20/11/2026", **Enviar teste agora** e
     **Desvincular**.
2. **O que avisar.** A mesma lista de regras da Análise (`rule-row`): uma frase,
   o limite quando o tipo tem (campo numérico, desligado junto do interruptor),
   quando o tipo avisa no ⓘ colado na frase e o interruptor à direita. Sem regra de IR para o
   ano de hoje, o isentômetro aparece **desligado e travado**, com a razão
   escrita ("o Prumo ainda não tem a regra de IR deste ano").
3. **Quando e com quê.** Hora do resumo (`<input type="time">`, horário de
   Brasília), o interruptor **Sem valores no resumo** ("só nomes e
   percentuais" à vista; no ⓘ, que o resumo passa pelos servidores do Telegram
   e que o olho só esconde a tela) e **Salvar alertas**. Erros
   ficam ao lado do campo (`role="alert"`) e travam o botão; "Descartar
   mudanças" volta ao salvo.
4. **Como o resumo fica.** Um exemplo, com dados **fictícios** (dito no ⓘ), só dos tipos
   ligados, no texto exato do servidor (com acento, como chega no Telegram). Acompanha
   "Sem valores no resumo" na hora, antes de salvar.

Com o olho fechado (ver "Ocultar valores"): a tela só tem valor em reais no
exemplo do resumo, e ali cada `R$` vira `R$ ••••`, inclusive para o leitor de
tela. O olho **não** muda o que o servidor envia: isso é "Sem valores no
resumo".

### 12. Ajustes › Proventos automáticos

Seção `#/ajustes/proventos-automaticos` (uma linha em `SETTINGS_SECTIONS`).
Responde "o Prumo calcula meus proventos?" em painéis lado a lado (`Grid
cols={2}`, uma coluna no celular); a regra está em
[proventos-automaticos.md](proventos-automaticos.md). A explicação da tela e a
frase do que ligar e desligar faz estão nos ⓘ.

1. **Calcular pela B3.** O interruptor **Proventos automáticos** (`role="switch"`)
   com "Ligados"/"Desligados". Ligado, a situação da busca ("Anúncios da B3
   conferidos em 26/11/2026 09:00. A busca é diária." ou "A primeira busca na
   B3 acontece em até uma hora."), os códigos acompanhados e os que ficaram sem
   anúncio. Trocar a chave grava na hora, com aviso. Desligado, o painel ocupa a
   linha inteira.
2. **Eventos em ações** (ao lado, só ligado). As pendências, na mesma frase do
   painel da Renda, ou "Nenhuma bonificação, desdobramento ou grupamento
   pendente".
3. **Como funciona** (recolhido, com a contagem: 6). Os limites em seis linhas:
   data com, IR do JCP, data a definir, 12 a 18 meses de histórico, o que não
   duplica (e que apagar é definitivo) e ETF/ação como "Outro".

Com o olho fechado, a quantidade da data com vira `••` ("Você tinha •• cotas
na data com"), sem singular que entregue o número; o fator do evento é
percentual e fica.

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
- **Alertas**: o exemplo do resumo mostra `R$ ••••` no lugar de cada valor. O
  olho só protege a tela; o que o servidor manda ao Telegram é decidido por
  "Sem valores no resumo" (ver [alertas.md](alertas.md)).
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
- Ícone do ativo: só enfeite (`aria-hidden`, `alt` vazio); nenhuma informação
  depende dele. Iniciais com contraste ≥ 4,5:1 nos dois temas.
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
