# Importar o extrato da B3

A Área do Investidor da B3 (investidor.b3.com.br) exporta planilhas `.xlsx` com
o que a pessoa negociou, o que passou pela custódia dela e os proventos. É dado
dela, baixado por ela: não há contrato, credencial nem raspagem. O Prumo lê
essas planilhas e mostra o que entraria na carteira.

> **Estado desta versão: só leitura e prévia.** `POST /api/v1/imports/b3/preview`
> lê, normaliza e devolve a prévia. Nada é gravado. A gravação (`commit`) chega
> na próxima onda, quando o livro de lançamentos e os proventos estiverem na
> `main`; o botão "Importar" da tela fica desligado até lá.

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

## Formato normalizado que a gravação vai usar (onda 2)

As linhas já saem no formato que encaixa nas tabelas das frentes F1 e F3. O
`external_ref` vira a chave única de cada tabela.

| Linha normalizada | Destino | Campos |
|---|---|---|
| `TradeLine` | `transactions` (F1), tipo `buy` ou `sell` | `date`, `side`, `code` (símbolo do ativo), `quantity`, `unit_price`, `total`, `institution`, `external_ref`; `market` (`spot`/`fractional`) é informativo |
| `EventLine` `split`, `reverse_split`, `bonus`, `amortization`, `redemption` | `transactions` (F1), tipos de evento | `date`, `kind`, `direction`, `quantity`, `value`, `unit_price`, `external_ref`; o desdobro traz as cotas **creditadas** (quantidade a somar), não o fator |
| `EventLine` `fraction_in_kind`, `maturity` | `transactions` (F1) ou conferência manual | idem |
| `EventLine` `treasury_movement` | frente do Tesouro | `treasury: true`, sem `code`; `direction` (crédito/débito) e valor como a B3 escreve |
| `IncomeLine` | `income_events` (F3) | `kind`, `status` (`received`/`provisioned`), `date` (pagamento; pode ser nula em provisionado), `code`, `net_amount` (sempre líquido), `quantity`, `unit_price` (por cota, bruto na maioria dos eventos), `institution`, `external_ref` |

Pontos que a onda 2 precisa decidir ou tratar:

- **Provisionado vira recebido.** O provento a receber e, depois, o recebido
  só têm o mesmo `external_ref` se data e valor líquido coincidirem. Quando
  não coincidirem, a gravação deve substituir o provisionado pelo recebido do
  mesmo código e tipo, em vez de somar os dois.
- **Ativo novo.** A linha traz `suggested_class` e `listed_on_b3` (sugestões).
  Ativo que ainda não existe na carteira nasce com a quantidade do livro, com a
  classe que a pessoa confirmar na tela.
- **Saldo inicial.** A planilha começa em ~nov/2019. Quem comprou antes precisa
  lançar um saldo inicial (F1) para a posição bater.
- **Preço médio.** Sem corretagem e emolumentos, o preço médio fica um pouco
  abaixo do fiscal; a tela deve deixar a pessoa ajustar a taxa.

## Privacidade

Os arquivos são a carteira inteira da pessoa:

- ficam só na memória do pedido, lidos de `&[u8]`: **nunca vão para disco**;
- **nada do conteúdo vai para o log**: nem nome de ativo, nem quantidade, nem
  nome de corretora. Um teste captura o log de um envio e confere;
- erro de leitura **não ecoa conteúdo**: diz só o tipo do problema;
- a prévia não grava nada em banco;
- a leitura de cada arquivo roda **isolada** (`catch_unwind`), com `panic = "unwind"`
  no perfil de release: um pânico da dependência que abre o `.xlsx` não derruba o
  processo, vira `422 unreadable_file` com só o nome do arquivo. Dentro da leitura
  o gancho de pânico registra apenas o local no código, nunca a mensagem, que
  às vezes cita o dado;
- a rota exige autenticação (`401` sem credencial) e a resposta sai com
  `Cache-Control: no-store`.

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
- **Sem corretagem nem taxas.** O preço médio sai um pouco abaixo do fiscal.
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
`backend/src/b3_xlsx.rs` (abrir o `.xlsx` na memória),
`backend/src/http/b3_import.rs` (a rota), `frontend/src/features/importB3/`
(a tela).
