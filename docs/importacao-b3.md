# Importar o extrato da B3

A Área do Investidor da B3 (investidor.b3.com.br) exporta planilhas `.xlsx` com
o que a pessoa negociou, o que passou pela custódia dela e os proventos. É dado
dela, baixado por ela: não há contrato, credencial nem raspagem. O Prumo lê
essas planilhas, mostra o que entraria na carteira e, quando a pessoa confere e
aperta **Importar**, grava tudo de uma vez.

- `POST /api/v1/imports/b3/preview` lê, normaliza, confronta com a carteira e
  devolve a prévia com o **plano** (o que é novo, o que já existe, o que
  conflita, o saldo inicial sugerido). Nada é gravado.
- `POST /api/v1/imports/b3/commit` grava o plano conferido, tudo ou nada.
  Reimportar o mesmo arquivo não duplica. Detalhes em [Gravar](#gravar).

## As quatro planilhas

A B3 não publica o esquema desses arquivos. Os cabeçalhos abaixo foram tirados
de leitores abertos que processam as exportações reais e de planilhas de
exemplo deles, e conferidos entre as fontes (lista no fim). A B3 pode mudar o
layout sem aviso: por isso o Prumo detecta o tipo pelos **cabeçalhos**, não pelo
nome do arquivo nem da aba, e diz com clareza o que não reconheceu.

| Planilha | Cabeçalhos | O que traz |
|---|---|---|
| **Negociação** | `Data do Negócio`, `Tipo de Movimentação`, `Mercado`, `Prazo/Vencimento`, `Instituição`, `Código de Negociação`, `Quantidade`, `Preço`, `Valor` | compras e vendas, uma linha por negócio, de todas as corretoras |
| **Movimentação** | `Entrada/Saída`, `Data`, `Movimentação`, `Produto`, `Instituição`, `Quantidade`, `Preço unitário`, `Valor da Operação` | todo débito e crédito da custódia: liquidações, proventos, eventos societários, Tesouro Direto |
| **Proventos recebidos** | `Produto`, `Pagamento`, `Tipo de Evento`, `Instituição`, `Quantidade`, `Preço unitário`, `Valor líquido` | o que já caiu na conta |
| **Proventos a receber** (provisionados) | `Produto`, `Tipo`, `Tipo de Evento`, `Previsão de pagamento`, `Instituição`, `Conta`, `Quantidade`, `Preço unitário`, `Valor líquido` | o que foi anunciado, com a data prevista (pode vir `-`) |

Convenções do formato, todas tratadas:

- Datas em texto `dd/mm/aaaa` (também aceita data do Excel e `aaaa-mm-dd`).
- `-` ou célula vazia quer dizer "sem valor".
- Números como número do Excel (ponto flutuante, `10.120000000000001` vira
  `10,12`) ou como texto pt-BR (`1.234,56`, `R$ 12,5`); `1.000` é mil.
- Linha em branco e rodapé (`Total`, `Total líquido`) são ignorados.
- Linhas do mais novo para o mais velho: a ordem não importa.
- Coluna extra, acento diferente, caixa diferente e espaço a mais nos
  cabeçalhos são tolerados. Faltando coluna obrigatória, o arquivo é recusado
  com a lista do que falta.
- Título antes do cabeçalho (até 15 linhas) é tolerado.

## Como o Prumo lê cada linha

### Produto, código e classe

O "Produto" da B3 vem como `PETR4 - PETROLEO BRASILEIRO S.A. PETROBRAS`. O
código é a primeira palavra (quatro caracteres e um ou dois dígitos). A
Negociação só traz o código, sem nome, e o fracionário termina em `F`
(`PETR4F`), que sai do código.

A classe e o `listed_on_b3` são **sugestões**, pelas mesmas regras da busca de
símbolos (`classify_b3`, em `quotes.rs`): BDR, ETF (cripto, renda fixa, S&P 500,
IFIX…), FII e ação. Quando o nome do produto aparece (Movimentação e Proventos),
a sugestão usa o nome (`class_basis: "name"`). Quando só há o código
(`class_basis: "code"`), o palpite é pelo formato e a tela pede para conferir:
um ETF terminado em `11` sem nome parece FII. Quem confirma é a pessoa.

**Tesouro Direto** aparece só na Movimentação, como `Tesouro Selic 2029`, sem
código. O Prumo o reconhece como renda fixa (`class_basis: "treasury"`,
`treasury: true`, `listed_on_b3: false`) e marca os movimentos como
`treasury_movement`, com quantidade, preço e valor como a planilha traz. O
preço e a posição do Tesouro são tratados pela frente do Tesouro.

### Negociação

| Coluna | Regra |
|---|---|
| `Tipo de Movimentação` | `Compra` → `buy`, `Venda` → `sell`; outro valor, linha não entendida |
| `Mercado` | `Mercado à Vista` (`spot`) e `Mercado Fracionário` (`fractional`); opções, termo e futuro ficam como não entendidos (ainda não suportados) |
| `Valor` | se vier vazio, é quantidade × preço arredondado em 2 casas |
| quantidade, preço | precisam ser maiores que zero |

Corretagem e emolumentos **não vêm** na planilha.

### Movimentação

| `Movimentação` | Vira |
|---|---|
| `Dividendo`, `Rendimento`, `Reembolso`, `Juros Sobre Capital Próprio` | provento (`income`), valor líquido; o JCP já vem com os 15% de IR descontados |
| `Leilão de Fração` | provento do tipo `fraction_auction` |
| `Desdobro` | evento `split` (a planilha traz as cotas creditadas, não o fator) |
| `Grupamento` | evento `reverse_split` |
| `Bonificação em Ativos` | evento `bonus` |
| `Fração em Ativos` | evento `fraction_in_kind` |
| `Amortização`, `Restituição de Capital` | evento `amortization` (devolve dinheiro, reduz o custo) |
| `Resgate`, `Vencimento` | evento `redemption`, `maturity` |
| qualquer movimento de **Tesouro Direto** | evento `treasury_movement` |
| `Transferência - Liquidação`, `Compra`, `Venda`, `Compra / Venda` de papel | **deixada de fora**: a compra ou venda vem da Negociação (a liquidação também registra empréstimo, a preço de referência) |
| `Transferência` | deixada de fora (custódia entre corretoras) |
| `... - Transferido` | deixada de fora (repete outro lançamento) |
| `Empréstimo` | deixada de fora (aluguel de ativos) |
| `Atualização` | **não entendida**: pode ter mudado a quantidade (fusão), confira |
| `Direitos de Subscrição`, `Cessão de Direitos` | **não entendida**: ainda não suportado |
| qualquer outra | **não entendida** |

Para o histórico completo de compras e vendas, **envie a Negociação**: sem ela,
a Movimentação sozinha não dá os negócios.

### Proventos

`Tipo de Evento` aceito: `Dividendo`, `Juros Sobre Capital Próprio`,
`Rendimento`, `Reembolso` (inclui `Reembolso - Juros Sobre Capital Próprio`) e
`Leilão de Frações`. Produto sem código (renda fixa privada) e evento fora
dessa lista (`PAGAMENTO DE JUROS`) são não entendidos. Em Proventos recebidos a
data de pagamento é obrigatória; em provisionados pode faltar.

## Linhas que não entram: duas listas

- **Não entendidas** (`unrecognized`): o Prumo não soube o que fazer. Cada uma
  traz arquivo, aba, linha, o motivo (`reason_code`) e as células preenchidas,
  para a pessoa decidir e lançar à mão.
- **Deixadas de fora de propósito** (`ignored`): o Prumo entendeu e não importa
  por um motivo claro (liquidação que a Negociação já cobre, transferência de
  custódia, empréstimo, linha repetida). Vêm contadas por motivo.

## `external_ref`: a identidade que torna a importação idempotente

Toda linha normalizada leva `external_ref`: `b3:v1:<trade|event|income>:<32 hex>`
(SHA-256 do conteúdo, 128 bits). A onda de gravação usa o texto como chave
única, e reimportar o mesmo arquivo não duplica.

- **Sem o nome do arquivo.** Períodos que se sobrepõem em dois arquivos dão o
  mesmo `external_ref` para o mesmo negócio.
- **Linhas idênticas num arquivo** (duas compras iguais no mesmo dia, comum no
  fracionário) recebem um contador de ocorrência e continuam distintas. Num
  segundo arquivo com só a primeira ocorrência, o `external_ref` dessa primeira
  é o mesmo.
- **Provento sem origem e sem corretora**: o mesmo pagamento na Movimentação e
  nos Proventos recebidos é uma linha só. Na prévia, quando o mesmo
  `external_ref` aparece em mais de um arquivo do envio, vale o primeiro por
  prioridade (Negociação, Proventos recebidos, Movimentação, Proventos a
  receber) e os outros viram `duplicate_in_request`.
- O `v1` muda só se a composição do hash mudar.

O conteúdo do hash: negócio = data, lado, mercado, código, quantidade, preço,
valor e corretora; evento = data, tipo, direção, produto, quantidade, valor,
preço e corretora; provento = tipo, código, data de pagamento e valor líquido.

## O que a prévia devolve

`POST /api/v1/imports/b3/preview` (multipart, um ou mais arquivos). Contrato
completo em [`api.md`](api.md#post-apiv1importsb3preview). Resumo: `files`
(o que cada arquivo foi e quantas linhas rendeu), `assets` (agrupado por ativo,
com classe sugerida, totais e as linhas), `unrecognized`, `ignored` e `summary`.

## Gravar

### Sem estado entre a prévia e a gravação

A prévia não guarda nada: nem os arquivos, nem o plano. Para gravar, a tela
**reenvia os mesmos arquivos** (que só existem na memória do navegador), as
decisões da pessoa e o `digest` do plano que ela conferiu. O servidor relê os
arquivos, abre uma transação, trava a carteira da pessoa (advisory lock da
pessoa + `for update` nos ativos, o mesmo lock do livro), refaz o plano ali
dentro e grava só se o `digest` for o mesmo. Se a carteira mudou no meio (um
"Já comprei", outro ativo), o plano muda, o `digest` muda e a resposta é
`409 preview_stale`: nada gravado, a tela lê de novo.

Foi a opção mais simples que é segura: guardar a prévia como lote temporário
no banco poria a carteira inteira da pessoa (os arquivos ou as linhas lidas)
em disco por um tempo, com limpeza e expiração para cuidar. Reenviar custa
subir os arquivos duas vezes (até 10 MiB) e não deixa nada para trás. O que
fica gravado é só o que a pessoa importou.

O `digest` é o SHA-256 (128 bits) da forma canônica do plano: cada ativo, o
destino de cada linha e tudo o que seria escrito. As mesmas planilhas, as
mesmas decisões e a mesma carteira dão sempre o mesmo `digest`.

### O destino de cada linha

| Estado | Quando |
|---|---|
| **novo** (`new`) | entra no livro ou na Renda |
| **já existe** (`existing`) | o `external_ref` já está no livro ou na Renda (`same_ref`); ou o lançamento digitado bate (`same_entry`); ou o provento já está lá (`same_income`) |
| **conflita** (`conflict`) | o livro recusaria: venda além da posição com o saldo inicial escolhido (`exceeds_position`), quantidade que a unidade do ativo não aceita (`unit`) |
| **fica de fora** (`not_imported`) | com o motivo: evento que não mexe na posição, título que não deu para identificar, provisionado sem data, ativo que a pessoa deixou de fora |

Conflito trava a gravação (`422 import_blocked`) até a pessoa ajustar o saldo
inicial ou deixar o ativo de fora. Classe só pelo código também trava até ela
confirmar.

**O "Já comprei" que também está na B3.** Um lançamento digitado (compra do
aporte, lançamento manual) sem `external_ref`, do mesmo ativo, tipo, data e
quantidade, e com preço até **2%** de diferença (ou R$ 0,01), é o mesmo
negócio: a linha da B3 fica como "já existe" e o lançamento digitado **ganha o
`external_ref`** da linha, para a próxima importação o achar direto. O preço
do aporte (o do plano) não é trocado pelo do pregão. Cada lançamento casa com
uma linha só.

### Negócios e eventos → livro

Tudo entra pelas funções do livro (`insert_row` + `rebuild`, na mesma
transação), com `source = b3_import` e o `external_ref` da linha.

| Linha | Lançamento |
|---|---|
| Negociação `Compra` / `Venda` | `buy` / `sell`, quantidade e preço da planilha, sem taxa |
| `Desdobro` (crédito) | `split` com as **cotas creditadas**: a B3 manda as cotas novas, não o fator, e é isso que o livro soma (custo igual) |
| `Grupamento` só com crédito | a B3 credita as **cotas novas** depois do grupamento: o livro grava `reverse_split` com **posição naquela data − cotas creditadas** (as que deixam de existir) |
| `Grupamento` com crédito e débito no mesmo dia | `reverse_split` com **débito − crédito**, um lançamento só para as duas linhas (`external_ref` próprio, do par) |
| `Grupamento` só com débito | `reverse_split` com as cotas debitadas |
| `Bonificação em Ativos` (crédito) | `bonus` com as cotas; o `Preço unitário`, quando vem, é o custo atribuído por cota |
| `Amortização`, `Restituição de Capital` | `amortization`: a quantidade de cotas e o valor por cota (sem o preço, valor ÷ cotas; sem as cotas, todas as da posição naquela data) |
| `Resgate`, `Vencimento` (fora do Tesouro) | `redemption`, com o preço da planilha ou valor ÷ quantidade |
| `Fração em Ativos` | fica de fora (`fraction_in_kind`): é a sobra que vai a leilão; o dinheiro entra como provento (`Leilão de Fração`) |
| desdobro, bonificação ou amortização como **débito** | fica de fora (`event_debit`), para a pessoa conferir |

No mesmo dia, o que soma entra antes do que tira (compra antes de venda):
day trade não vira "venda além da posição".

### Tesouro Direto

A B3 escreve o título como `Tesouro Selic 2029`: o tipo e o **ano**, sem o
dia do vencimento. O Prumo liga ao título (`Market::Treasury`,
`TD-<tipo>-<vencimento>`) quando há **um só** daquele tipo e ano na carteira
da pessoa ou, se ela não tem, na tabela do Tesouro Transparente que o Prumo
já baixa. Sem isso (título que saiu da tabela e a pessoa nunca cadastrou, ou
dois do mesmo ano), as linhas ficam de fora com `treasury_unidentified`.

| Movimento | Lançamento |
|---|---|
| crédito com quantidade (o título entrou na custódia: compra) | `buy` |
| débito com quantidade (`Resgate`, `Vencimento`) | `redemption` |
| outro débito com quantidade (venda) | `sell` |
| sem quantidade (juros, cupom) | fica de fora (`treasury_interest`): lance como provento à mão |

O preço é o da planilha ou valor ÷ quantidade; a quantidade, em centésimos.

### Proventos → Renda

| B3 | `income_events.kind` |
|---|---|
| `Dividendo` | `dividend` |
| `Juros Sobre Capital Próprio` | `jcp` |
| `Rendimento` | `reit_income` |
| `Reembolso`, `Leilão de Frações` | `other` |

`source = b3_import`, `external_ref` da linha, sem data ex (a B3 não traz).
O provento importado **vence o automático** (`b3_announcement`, ver
[proventos-automaticos.md](proventos-automaticos.md)): mesma data de
pagamento, mesmo tipo e o mesmo valor por cota a 1% (no JCP vale também o
líquido por cota) — a linha sai `replaces_automatic` e a gravação troca o
automático pelo importado no mesmo registro.
O valor da B3 é o **líquido**. No JCP o bruto é **líquido ÷ (1 − alíquota)**,
com a alíquota retida na fonte da data do pagamento — 15% até 2025, 17,5% desde
2026 (LC 224/2025; provisionado sem data já entra na de 2026) — e a retenção é a
diferença; nos outros, bruto = líquido.
Quantidade: a da B3 quando ela traz; sem ela, as cotas que o livro tem na
**data com** (a regra de `GET /income/entitlement`: sem data ex, a data do
pagamento, nunca depois de hoje), já contando os lançamentos desta mesma
importação. Valor por cota: o `Preço unitário` da planilha, ou líquido ÷
quantidade.

O provento se liga ao ativo da carteira (o que já existe ou o que a importação
cria). Provento de ativo que não está na carteira e não ganha lançamento entra
sem ativo ligado, com o código e o nome (como o de um ativo removido).

**Provisionado → recebido: substitui, não soma.** O provento a receber fica
marcado como provisório (`income_events.provisional`, migração 0011). A
regra do "mesmo provento":

- mesmo ativo e mesmo tipo;
- pagamento até **20 dias** de distância;
- o mesmo **valor por cota** (é o que a empresa anuncia e não muda; o líquido
  muda com a quantidade e o arredondamento). Sem o valor por cota de um dos
  lados, só a mesma data de pagamento.

Quando o recebido chega, ele **toma o lugar** do provisório mais perto na data
(a mesma linha no banco é atualizada: data, valores, `external_ref` e
`provisional = false`). Um "a receber" novo do mesmo provento atualiza o
provisório também. Um "a receber" que chega depois do recebido (arquivo velho)
é "já existe". No mesmo envio, o recebido vence o provisionado. Provisionado
**sem data** ("-") fica de fora (`provisioned_without_date`): entra quando a
B3 marcar a data. Um provento digitado à mão com o mesmo ativo, tipo, data de
pagamento e líquido (até R$ 0,01) é "já existe".

### Ativo novo e classe

Ativo que ainda não está na carteira nasce com a classe sugerida
(`classify_b3`, `listed_on_b3: true`) e quantidade do livro. Quando a classe
veio só do código (`class_basis: code`), a prévia marca
`class_needs_confirmation` e a gravação espera a pessoa confirmar ou trocar
(`asset_class` nas decisões). Ativo que já existe fica com a classe dele.
Ativo só com proventos e nenhum lançamento novo não é criado.

Ativo que a B3 mostra inteiro vendido entra com quantidade zero: o histórico
conta para o preço médio e o IR. A pessoa pode removê-lo da carteira depois; os
lançamentos ficam.

### Saldo inicial

A B3 começa em ~nov/2019. Para cada ativo com lançamento novo, o plano monta
**um** saldo inicial (`opening`), datado na véspera do primeiro lançamento do
livro, que **substitui os saldos iniciais e os ajustes de quantidade que já
estavam no livro** (as quantidades que a pessoa digitou no cadastro ou na
edição, e as que a migração 0010 gravou). A quantidade sugerida é a maior
destas:

1. **Cobrir as vendas** (`covers_sales`): o mínimo para nenhuma venda,
   grupamento ou amortização passar da posição em data nenhuma.
2. **Manter a quantidade digitada** (`keeps_quantity`): a quantidade que o
   livro tinha na data do **último** saldo/ajuste digitado, menos o que o
   histórico (B3 e lançamentos de verdade) explica até essa data. Se a B3
   explica tudo, o saldo inicial some; se explica parte, o resto fica.

Exemplo: a pessoa cadastrou 100 cotas em maio de 2026. Com a Negociação desde
2019 (60 + 40 antes de maio), o saldo inicial some e o custo fica conhecido.
Com só a de 2026 (uma compra de 10 em julho), o saldo inicial fica com 100 e
a posição vai a 110. Se a B3 tiver **mais** do que a pessoa digitou, a B3
vence (o saldo inicial não fica negativo).

O preço médio sugerido é o dos saldos iniciais substituídos, quando todos
tinham; senão, custo desconhecido. A pessoa pode trocar a quantidade e o preço
médio (`opening` nas decisões; zero = sem saldo inicial). É a **única entrada
manual** do fluxo. O lançamento vai com `source = b3_import` e sem
`external_ref`.

### Tudo ou nada

A gravação inteira é uma transação: ativos, lançamentos, refação da posição,
proventos. Se o livro recusar algo, um ativo nascer repetido no meio ou o
banco falhar, nada fica. Reimportar o mesmo arquivo, ou um período que se
sobrepõe, grava zero: tudo já existe pelo `external_ref`.

### Depois de gravar

- **Carteira:** a posição em cache (`assets.quantity`, `cost_basis`,
  `realized_gain`) é refeita na mesma transação; a Carteira e o stream ao vivo
  já leem o novo.
- **Desempenho:** a importação não mexe na série diretamente. Os gatilhos
  da migração 0013 marcam a série da pessoa como suja desde a menor data que
  a gravação escreveu ou apagou (o negócio de 2019, o saldo inicial
  substituído, o provento trocado), e a tarefa em segundo plano refaz a série
  **inteira** dali em diante, a partir do livro. Refazer só a foto de hoje
  seria redundante (a tarefa a sobrescreve) e deixaria o passado errado até
  lá; a tela mostra o aviso de reconstrução enquanto isso.
- **Caixa:** é calculado do livro e só conta provento recebido e venda a
  partir de `users.cash_since`. Importar anos de histórico não enche o caixa:
  só o que tem data de `cash_since` em diante entra (por exemplo, uma venda
  de ontem que o "Já comprei" não registrou).
- **Renda:** lê o livro de proventos direto, sem cache no servidor.
- **Ativos novos** buscam cotação e histórico de dividendos em segundo plano.
- **Tela:** invalida carteira, lançamentos, Renda, caixa, saúde e Desempenho.

## Privacidade

Os arquivos são a carteira inteira da pessoa:

- ficam só na memória do pedido, lidos de `&[u8]`: **nunca vão para disco**;
- **nada do conteúdo vai para o log**: nem nome de ativo, nem quantidade, nem
  nome de corretora. Um teste captura o log de um envio e confere;
- erro de leitura **não ecoa conteúdo**: diz só o tipo do problema;
- a prévia não grava nada em banco; a gravação grava só o que a pessoa
  importou (ativos, lançamentos, proventos), nunca o arquivo nem a linha crua;
- a leitura de cada arquivo roda **isolada** (`catch_unwind`), com `panic = "unwind"`
  no perfil de release: um pânico da dependência que abre o `.xlsx` não derruba o
  processo, vira `422 unreadable_file` com só o nome do arquivo. Dentro da leitura
  o gancho de pânico registra apenas o local no código, nunca a mensagem, que
  às vezes cita o dado;
- as rotas exigem autenticação (`401` sem credencial), cada pessoa só vê e
  grava o próprio livro (o `external_ref` é único por pessoa), e a resposta
  sai com `Cache-Control: no-store`.

## Limites

| Limite | Valor |
|---|---|
| Corpo do envio (todos os arquivos juntos) | 10 MiB; acima, `413 payload_too_large`. O resto da API continua em 64 KiB |
| Arquivos por envio | 10 |
| Descompactado (soma declarada no zip) | 64 MiB; acima, o arquivo é recusado antes de abrir (bomba de descompressão) |
| Entradas no zip | 1.000 |
| Abas por arquivo | 20 |
| Linhas por aba | 200.000 |
| Colunas lidas | 64 |
| Células por arquivo | 2.000.000 |

A leitura é em fluxo, célula a célula: uma planilha com uma célula perdida no
canto (`XFD1048576`) não aloca a grade inteira.

## Limitações

- **Histórico desde ~2019.** A Área do Investidor substituiu o CEI e traz dados
  a partir de novembro de 2019.
- **Sem corretagem nem taxas.** O preço médio sai um pouco abaixo do fiscal;
  a taxa pode ser lançada depois, editando o lançamento.
- **Juros e cupom do Tesouro** e **fração em ativos** não entram (ficam de
  fora com o motivo).
- **Opções, termo e futuro** não são lidos (são listados como não entendidos).
- **Subscrição, cessão de direitos e "Atualização"** (fusões, incorporações) não
  são lidos; a pessoa lança à mão.
- **Posição e consolidado mensal/anual** não são lidos nesta versão.
- **`.xls` antigo e CSV** não são aceitos: só `.xlsx`.
- A B3 pode mudar o layout. Cabeçalho novo vira mensagem clara com as colunas
  que faltam.

## Fontes dos formatos

Nenhum arquivo real foi usado: os testes montam planilhas fictícias (tickers
públicos, quantidades e valores inventados) no formato descrito acima.

- B3, Área do Investidor: <https://www.b3.com.br/pt_br/produtos-e-servicos/central-depositaria/canal-com-investidores/area-do-investidor/>
  (os dados da área logada começam em 01/11/2019; a movimentação é o histórico de
  débitos e créditos da conta).
- MarceloPCF/COIR, `coir/importers/b3.py`: cabeçalhos de Negociação e
  Movimentação, datas em texto, `-` como vazio, `Credito`/`Debito`.
- 4nier-app/patrimonio, `crates/providers/src/b3_extrato.rs`: mercado à
  vista/fracionário, rótulos da Movimentação.
- tbc174/tickerlab, `scripts/gerar_planilhas_de_exemplo.py`: planilhas
  fictícias dos três extratos, Proventos a receber (`Previsão de pagamento`,
  linha `Total líquido`), Tesouro Direto e Empréstimo.
- MuriloScarpaSitonio/multi-sources-financial-control,
  `integrations/b3/proventos.py`: aba Proventos Recebidos
  (`Produto`, `Pagamento`, `Tipo de Evento`, `Valor líquido`).
- larahfelipe/ex3, `docs/transaction-import.md`: colunas dos três extratos
  reunidas e a sobreposição entre Movimentação e Proventos.
- joao-sallaberry/portfolio-tracker, `src/lib/b3-movimentacao.ts`: vocabulário
  de movimentações (Fração em Ativos, Leilão de Fração, Atualização, Direitos
  de Subscrição, Cessão de Direitos).

Código: `backend/src/domain/b3_import.rs` (regras puras e modelo),
`backend/src/domain/b3_plan.rs` (o plano: estados, saldo inicial, eventos,
proventos), `backend/src/b3_xlsx.rs` (abrir o `.xlsx` na memória),
`backend/src/store/b3_import.rs` (a gravação), `backend/src/http/b3_import.rs`
(as rotas), `frontend/src/features/importB3/` (a tela).
