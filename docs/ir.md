# IR: isentômetro, apuração mensal, DARF e IRPF

> **Aviso de responsabilidade.** O Prumo calcula uma **estimativa** do imposto a
> partir do que a pessoa lançou. Não é orientação tributária e não substitui o
> contador, o programa da Receita nem o informe de rendimentos da corretora.
> Confira cada DARF e cada linha da declaração antes de pagar ou entregar. Erro
> de imposto gera multa e juros, e a responsabilidade pelo que vai para a
> Receita é de quem declara.

O cálculo mora em `backend/src/domain/tax.rs` e `backend/src/domain/tax/`
(domínio puro, sem banco nem rede). Esta página diz **que regra vale em cada
ano, de onde ela vem** e o que o Prumo **não** calcula.

- Fontes consultadas em **25/11/2026**; as do IR retido pela regra, da multa e
  dos juros do DARF em atraso e da Selic do mês (§§ 2.7 e 2.11), em
  **27/11/2026**.
- "P&R" é o **Perguntas e Respostas IRPF 2026** da Receita (v1.00, de
  23/04/2026; ano-calendário 2025), em
  [gov.br/receitafederal › Perguntas e respostas › DIRPF](https://www.gov.br/receitafederal/pt-br/centrais-de-conteudo/publicacoes/perguntas-e-respostas/dirpf/p-r-irpf-2026-v1-00-2026-04-23.pdf/view).
- Leis no [planalto.gov.br](https://www.planalto.gov.br/); a IN RFB
  1.585/2015 em [normas.receita.fazenda.gov.br](http://normas.receita.fazenda.gov.br/sijut2consulta/link.action?idAto=67494).
- "P&R PJ" é o **Perguntas e Respostas Pessoa Jurídica**, cap. XVIII
  (Acréscimos Legais), da Receita, em
  [gov.br/receitafederal › ECF](https://www.gov.br/receitafederal/pt-br/assuntos/orientacao-tributaria/declaracoes-e-demonstrativos/ecf/perguntas-e-respostas-pessoa-juridica-2021-arquivos/capitulo-xviii-acrescimos-legais-2021.pdf).

## 1. Como a regra fica no código

- Cada ano-calendário é uma função em `domain/tax/rules.rs` (`year_2025`,
  `year_2026`). Cada alíquota, limite, código de DARF, código da declaração,
  taxa do IR retido e acréscimo do DARF em atraso vem com a fonte ao lado
  (`Sourced { value, source }`). `GET /tax/rules/{year}` devolve a tabela
  inteira, e a tela mostra cada regra com a fonte ("O que o Prumo cobre").
- **Ano sem regra cadastrada é erro** (`TaxError::NoRulesForYear`). O Prumo
  nunca usa a regra de outro ano em silêncio. Hoje a tabela cobre **2025 e
  2026**. Vendas de anos anteriores só formam o preço médio e não são
  apuradas; o prejuízo acumulado até 31/12/2024 entra como saldo inicial
  (`Carryforward`).
- Ano novo entra como uma função nova, revisada contra a lei daquele ano, com
  teste. Não basta copiar o ano anterior.
- O calendário de dias úteis (`domain/tax/calendar.rs`) também é tabela por
  ano: 2025, 2026 e 2027 (o imposto de dezembro de 2026 vence em janeiro de
  2027). Ano fora da tabela é `TaxError::NoCalendarForYear`.

## 2. Regras de 2025 e 2026

As regras do ganho em bolsa e em cripto são **iguais nos dois anos**. O que
mudou de 2025 para 2026 foi o JCP e a retenção sobre dividendos altos (§ 2.9).

### 2.1 Ações — operação comum

- **Alíquota:** 15% sobre o ganho líquido do mês. Fonte: Lei 11.033/2004, art.
  2º, II; IN RFB 1.585/2015, art. 57; P&R 706.
- **Isenção:** o lucro das ações em operação comum é isento no mês em que o
  **total das vendas de ações** não passa de **R$ 20.000,00** ("igual ou
  inferior"; R$ 20.000,00 exatos ainda é isento). Fonte: Lei 11.033/2004, art.
  3º, I; IN RFB 1.585/2015, art. 59, I; P&R 707.
- O limite é sobre **valor vendido** (bruto, quantidade × preço), não sobre o
  lucro. Units contam como ações.
- **Prejuízo em mês isento continua compensável** com lucro tributável de
  meses seguintes (IN RFB 1.585/2015, art. 59, § 1º; P&R 729). O lucro isento
  não consome o prejuízo acumulado.
- Custo: média ponderada dos custos unitários, com corretagem e emolumentos
  somados à compra e abatidos da venda. A média é por ativo, mesmo em
  corretoras diferentes. Fonte: IN RFB 1.585/2015, arts. 56, § 3º, e 58; P&R
  708, 719 e 720.
- Desdobramento e grupamento não mudam o custo total (art. 58, § 7º, II; P&R
  722). Bonificação entra pelo valor capitalizado por ação que a empresa
  informa (P&R 721).

### 2.2 Day trade

- **O que é:** compra e venda do mesmo ativo no mesmo dia, na mesma
  instituição. A quantidade casada é day trade, casando a primeira compra com a
  primeira venda, sucessivamente. O estoque de antes do dia não entra. Fonte:
  IN RFB 1.585/2015, art. 65, §§ 1º a 3º; P&R 705.
- **Alíquota:** 20%. Fonte: Lei 11.033/2004, art. 2º, I; P&R 706.
- **Sem isenção:** o limite de R$ 20 mil não se aplica ao day trade (art. 65,
  § 15; P&R 707).
- **Compensação separada:** prejuízo de day trade só abate lucro de day trade,
  e prejuízo comum só abate lucro comum (art. 64; P&R 709).
- **IR retido de 1%** sobre o resultado positivo, pela corretora (Lei
  9.959/2000, art. 8º; P&R 712 e 713).
- O Prumo identifica o day trade sozinho, pelo dia e pelo ativo do
  lançamento (o livro não guarda a corretora: ver § 7.2). FII e Fiagro também
  têm day trade (o casamento tira a compra do preço médio), mas o resultado
  fica no grupo do próprio fundo, que já é 20%.
- A base do 1% retido é o resultado positivo **de cada dia**, com a perda de
  day trade do mesmo dia compensada (IN RFB 1.585/2015, art. 65, § 1º, II, e
  § 4º; P&R 705). Dia de prejuízo não retém nada e não abate o lucro de outro
  dia. Ver § 2.7 e a dúvida § 5.11.

### 2.3 FII e Fiagro

- **Venda de cotas:** 20% sobre o ganho líquido, **sem a isenção de R$ 20
  mil**. Fonte: Lei 8.668/1993, art. 18 (red. Lei 9.779/1999) para FII e art.
  20-D (Lei 14.130/2021) para Fiagro; IN RFB 1.585/2015, arts. 37 e 56, § 1º,
  I, "e".
- **Compensação:** prejuízo na venda de cotas só compensa com ganho de fundo
  **da mesma espécie** (IN RFB 1.585/2015, art. 37, § 2º). O Prumo mantém um
  grupo para FII e outro para Fiagro (ver dúvidas, § 5).
- **Rendimento distribuído:** isento para pessoa física quando as cotas são
  negociadas só em bolsa ou balcão organizado, o fundo tem **pelo menos 100
  cotistas**, a pessoa tem **menos de 10%** das cotas (ou dos rendimentos) e o
  grupo de cotistas ligados tem menos de 30%. Fonte: Lei 11.033/2004, art. 3º,
  III e § 1º (red. Lei 14.754/2023); P&R 738, VII.

### 2.4 ETF de renda variável e BDR

- **Alíquota:** 15% na operação comum, 20% no day trade, no mesmo grupo de
  compensação das ações.
- **Sem a isenção de R$ 20 mil:** a isenção é só para ações. ETF: IN RFB
  1.585/2015, art. 59, § 2º ("quotas dos fundos de investimento em índice de
  ações"); P&R 707. BDR: IN RFB 1.585/2015, art. 56, § 1º, I, "a" (BDR em bolsa
  é ganho líquido) e Lei 11.033/2004, art. 3º, I (isenção só para ações).
  Solução de Consulta Cosit 39/2022 no mesmo sentido.
- As vendas de ETF e BDR não somam no isentômetro de ações.

### 2.5 Criptoativos (no Brasil)

- **Isenção:** mês em que o **total alienado do conjunto de criptoativos**
  (todos os tipos: bitcoin, altcoins, stablecoins, NFT) não passa de **R$
  35.000,00**. Passou, todo o ganho do mês é tributado. Fonte: Lei 9.250/1995,
  art. 22, II (red. Lei 11.196/2005); P&R 653, item 2.
- **Alíquotas:** ganho de capital pela tabela progressiva: 15% até R$ 5
  milhões de ganho, 17,5% de R$ 5 a 10 milhões, 20% de R$ 10 a 30 milhões e
  22,5% acima. Fonte: Lei 8.981/1995, art. 21 (red. Lei 13.259/2016); P&R 574.
- **Sem compensação:** o ganho é apurado por alienação, e prejuízo de uma não
  abate o lucro de outra (IN SRF 84/2001, art. 2º, parágrafo único; P&R 583).
- **DARF 4600**, até o último dia útil do mês seguinte (P&R 653).
- Cripto custodiada ou negociada **no exterior** segue outra regra desde
  1/1/2024: aplicação financeira no exterior, 15% na declaração anual, sem
  isenção (Lei 14.754/2023, arts. 3º e 4º; P&R 653, item 1). O Prumo **não**
  calcula esse caso. Em custódia própria, vale a residência da pessoa (P&R
  473).

### 2.6 Compensação de prejuízo: o que compensa com o quê

| Grupo no Prumo | Entra | Alíquota | Prejuízo compensa com |
|---|---|---|---|
| `common` | ações, ETF e BDR em operação comum | 15% | lucro comum de ações, ETF e BDR |
| `day_trade` | day trade de ações, ETF e BDR | 20% | lucro de day trade |
| `real_estate_fund` | FII (comum e day trade) | 20% | lucro na venda de FII |
| `agribusiness_fund` | Fiagro (comum e day trade) | 20% | lucro na venda de Fiagro |
| — | cripto | 15% a 22,5% | não compensa |

- O prejuízo vale nos meses seguintes, sem prazo, e atravessa o ano: prejuízo
  de dezembro compensa em qualquer mês do ano seguinte (P&R 711).
- Prejuízo não volta para meses anteriores (P&R 710).
- A tela mostra o saldo de cada grupo **mês a mês**, na coluna "Prejuízo a
  compensar" da tabela dos meses: o que sobra no fim do mês, o que compensou
  lucro e o que entrou, mais o saldo de 1º de janeiro e o de 31/12. É o que
  vai, mês a mês, no Demonstrativo de Renda Variável (P&R 711).

### 2.7 IR retido na fonte ("dedo-duro")

- **Operação comum em bolsa:** a corretora retém **0,005%** do valor de cada
  venda no mercado à vista (ações, ETF, BDR, FII e Fiagro). A retenção do mês
  é a **soma** das operações; se a soma dá **R$ 1,00 ou menos**, nada é
  retido. Fonte: Lei 11.033/2004, art. 2º, § 1º, IV, e §§ 4º e 5º; IN RFB
  1.585/2015, art. 63, IV, e §§ 4º e 5º; P&R 706.
- **Day trade:** **1%** do resultado positivo de cada dia, com as perdas de
  day trade do mesmo dia compensadas (Lei 9.959/2000, art. 8º; IN RFB
  1.585/2015, art. 65, § 1º, II, e § 4º; P&R 705 e 712). O day trade não paga
  os 0,005%.
- Cripto não tem retenção (não é bolsa).
- O retido **abate o imposto do mês** em bolsa (código 6015). O que sobra passa
  para os meses seguintes **só dentro do ano-calendário**. A sobra de dezembro
  não vai para o ano seguinte: a do 0,005% compensa na declaração anual, e a
  do day trade pode ser pedida em restituição. Fonte: Lei 11.033/2004, art. 2º,
  § 7º; IN RFB 1.585/2015, art. 63, § 8º, e art. 65, §§ 8º e 9º; P&R 714 e 715.

**De onde vem o valor.** O livro não guarda a nota de corretagem. Por mês:

1. se a pessoa informou o IR retido do mês ("Informar o valor da nota", na
   apuração do mês), vale o dela (`tax_withholding_informed`);
2. senão, vale **a regra**: o que a corretora é obrigada a reter, pela conta
   acima, com as vendas do próprio livro. O mês leva o aviso
   `withholding_by_rule` ("confira na nota de corretagem").

A regra supõe **uma corretora só**, como o casamento do day trade: a dispensa
de R$ 1,00 e o day trade são por instituição, e com duas corretoras a conta
pode dar um pouco diferente da nota. É por isso que o valor da nota, quando
informado, substitui a regra. Exemplo (P&R 725, item 1): venda à vista de R$
130.000,00 com ganho de R$ 20.000,00: imposto de R$ 3.000,00, retenção de R$
6,50 (0,005%), DARF de R$ 2.993,50.

### 2.8 DARF

- **Código 6015** (ganho líquido em bolsa: comum, day trade, FII e Fiagro
  juntos num DARF só) e **4600** (ganho de capital: cripto). Fonte: P&R 730 e
  653.
- **Vencimento:** último dia útil do mês seguinte ao da apuração. Fonte: IN
  RFB 1.585/2015, art. 56, § 5º; P&R 730.
- **Valor mínimo:** DARF abaixo de **R$ 10,00** não é pago. O valor soma ao
  imposto do mesmo código dos meses seguintes até chegar a R$ 10,00, e então
  vence no prazo do último mês. Fonte: Lei 9.430/1996, art. 68, caput e § 1º.
  O acúmulo atravessa o ano.
- **Dia útil:** dia com expediente bancário. Fora sábado, domingo e feriado
  nacional (Leis 662/1949, 6.802/1980, 9.093/1995 e 14.759/2023), o calendário
  também fecha Carnaval, Corpus Christi e 31/12. Na dúvida, o vencimento vai
  para antes. A tabela foi conferida contra a
  [Agenda Tributária da Receita](https://www.gov.br/receitafederal/pt-br/assuntos/agenda-tributaria)
  (código 6015, jan/2025 a nov/2026). Por exemplo, o DARF de novembro de 2025
  venceu em **30/12/2025**.

### 2.9 Proventos: dividendos e JCP, e o que mudou

**JCP:** tributação exclusiva na fonte. O imposto retido não compensa na
declaração (P&R 320).

- **2025:** 15%. Fonte: Lei 9.249/1995, art. 9º, § 2º (redação até 2025); P&R
  320.
- **2026:** **17,5%**. Fonte: Lei 9.249/1995, art. 9º, § 2º, na redação da
  [LC 224/2025](https://www.planalto.gov.br/ccivil_03/leis/lcp/Lcp224.htm), art.
  8º, publicada em 26/12/2025 e com efeitos desde 1/1/2026 (art. 14, III).

**Dividendos:**

- **2025:** isentos, sem retenção (Lei 9.249/1995, art. 10).
- **2026:** continuam isentos na declaração, mas a
  [Lei 15.270/2025](https://www.planalto.gov.br/ccivil_03/_ato2023-2026/2025/lei/L15270.htm)
  (de 26/11/2025, efeitos desde 1/1/2026) criou o art. 6º-A da Lei 9.250/1995:
  - quando uma **mesma empresa** paga a uma pessoa **mais de R$ 50.000,00 no
    mês**, retém **10% sobre o total** (não só sobre o excedente);
  - lucro apurado até 2025, com distribuição aprovada até 31/12/2025 e paga nos
    termos aprovados, fica de fora;
  - a retenção é antecipação do **IRPF mínimo** (art. 16-A): a partir do
    ano-calendário 2026, para quem tem renda total acima de R$ 600 mil no ano,
    alíquota de até 10%. A base inclui os ganhos líquidos em bolsa; ficam de
    fora, entre outros, a poupança e os rendimentos de FII e Fiagro com pelo
    menos 100 cotistas e cotas só em bolsa.
- Os pagamentos do mês da mesma empresa somam (P&R 276): o Prumo junta os
  dividendos por **CNPJ** (ON e PN da mesma empresa são um pagador só, § 2.10)
  e marca o mês que passou de R$ 50 mil, com os 10% que a regra manda reter e
  o retido que veio no provento, lado a lado. A exceção do lucro de até 2025
  aprovado até 31/12/2025 o Prumo não tem como saber: por isso a retenção da
  regra é só mostrada, nunca somada.
- **Não calcula o IRPF mínimo.**

**O que caducou: MP 1.303/2025.**

- A [MP 1.303](https://www.planalto.gov.br/ccivil_03/_ato2023-2026/2025/Mpv/mpv1303.htm),
  de 11/06/2025, unificaria em 17,5% a tributação de aplicações financeiras e
  de cripto a partir de 2026. Ela também acabaria com a isenção de R$ 20 mil e
  com a de R$ 35 mil, taxaria em 5% os rendimentos novos de FII e Fiagro e
  levaria o JCP a 20%.
- Ela **perdeu a eficácia em 8/10/2025**, depois de retirada de pauta na
  Câmara (Ato do Presidente do Congresso Nacional 67/2025), e não foi
  convertida em lei. Como os efeitos só começariam em 2026, nada dela vale em
  2025 nem em 2026.
- O único ponto que voltou foi o JCP, e com outra alíquota (17,5%), por outra
  lei (LC 224/2025).

### 2.10 IRPF: Bens e Direitos e rendimentos

**Bens e Direitos**, pelo **custo de aquisição** em 31/12 (não pelo valor de
mercado):

| Instrumento | Grupo | Código |
|---|---|---|
| Ações e units | 03 | 01 |
| FII | 07 | 03 |
| Fiagro | 07 | 02 |
| ETF de renda variável | 07 | 09 |
| BDR | 04 | 04 |
| Bitcoin | 08 | 01 |
| Outras criptomoedas (altcoins) | 08 | 02 |
| Stablecoins (USDT, USDC, BRZ, DAI...) | 08 | 03 |

- Os códigos de cripto vêm da P&R 473.
- Os demais vêm do programa IRPF 2026 e só foram confirmados em fonte
  secundária (informes de corretoras). Para 2026, são os mesmos até a Receita
  publicar o programa IRPF 2027. Ver dúvidas (§ 5).

**Rendimentos:**

- **Isentos:**
  - `09`: lucros e dividendos;
  - `20`: lucro de ações em mês com vendas até R$ 20 mil;
  - `05`: ganho em cripto em mês com alienações até R$ 35 mil;
  - rendimento de FII e Fiagro, informado em "Outros" (`99`).
- **Tributação exclusiva:** `10`, JCP, pelo valor líquido.
- **Renda variável:** os meses apurados vão para o "Demonstrativo de Apuração
  de Ganhos – Renda Variável". O prejuízo a compensar em 31/12 fica para o ano
  seguinte (P&R 711 e 729).

**Por fonte pagadora.** A ficha de rendimentos pede, em cada linha, o CNPJ e o
nome de quem pagou. O Prumo soma cada código por fonte pagadora e preenche os
dois do cadastro da CVM que ele já guarda para os fundamentos:

- **companhia** (dividendo e JCP): o emissor do ticker (as quatro letras) dá o
  código CVM que a B3 informa (`b3_issuer_cvm`), e o código dá o CNPJ e a razão
  social no cadastro de companhias abertas (`cvm_companies`,
  `cad_cia_aberta.csv`);
- **FII e Fiagro** (rendimento): o fundo do ticker pelo caminho único de
  ticker de FII → fundo (`Store::fund_cnpjs`): o CNPJ que a B3 lista para a
  sigla ou, sem ela, o desempate entre os fundos que declaram a raiz de ISIN no
  informe mensal ([`fundamentos.md`](fundamentos.md), "Ticker de FII →
  fundo"), com a razão social do informe (ou da B3, quando o fundo dela não
  está no informe: `payer_source: "b3_fund"`). É o CNPJ do fundo (ou da
  classe, depois da Resolução CVM 175). Antes, valia o informe mais novo da
  raiz de ISIN, e um fundo que preencheu o ISIN de outro levava o CNPJ errado
  ao informe de rendimentos.

Tickers com o mesmo CNPJ somam numa linha só. Sem cadastro (ativo que nunca
teve os fundamentos buscados, emissor fora da CVM), a linha sai pelo ticker,
escrito "CNPJ não encontrado": confira no informe de rendimentos. Cada linha
tem "Copiar" (ficha, código, CNPJ, nome e valor), e "Copiar o relatório" leva
todas de uma vez.

### 2.11 DARF pago em atraso: multa e juros

- **Multa de mora:** **0,33% por dia** de atraso, contado do dia seguinte ao
  vencimento até o dia do pagamento (dias corridos), com teto de **20%**.
  Fonte: Lei 9.430/1996, art. 61, caput e §§ 1º e 2º; P&R PJ, cap. XVIII, 001.
- **Juros de mora:** a **soma** da Selic de cada mês (a "acumulada
  mensalmente": as taxas mensais somadas, não compostas), do mês seguinte ao do
  vencimento até o mês anterior ao do pagamento, mais **1%** no mês do
  pagamento, sobre o imposto **sem** a multa. Fonte: Lei 9.430/1996, art. 61,
  § 3º, e art. 5º, § 3º; P&R 064; P&R PJ, cap. XVIII, 001.
- Os juros só começam no primeiro dia do mês seguinte ao do vencimento: pago
  ainda no mês do vencimento (o 6015 vence no último dia útil, então só sobra o
  fim de semana ou o 31/12), há multa e **não há juros**.
- O vencimento é o do DARF do mês; o DARF que somou meses abaixo do mínimo
  vence no prazo do último deles (Lei 9.430/1996, art. 68, § 1º), que é o que
  o Prumo já mostra.
- **A Selic do mês:** série **4390** do SGS do Banco Central (Selic acumulada
  no mês, % a.m.), em
  `https://api.bcb.gov.br/dados/serie/bcdata.sgs.4390/dados?formato=json`. É a
  taxa da tabela de acréscimos legais da Receita: o teste
  `interest_matches_the_receita_table_for_july_2026` refaz a tabela de juros
  para pagamento em julho de 2026 (a da Receita, republicada pelo CNB/SP) (vencimentos de janeiro de 2025 a junho de
  2026, de 20,11% a 1,00%) somando a série, e bate em todos os meses.
- O SGS publica o mês corrente acumulado até a véspera (em 27/11/2026, o de
  novembro dizia 0,87%). O Prumo só guarda **mês fechado há 4 dias**
  (`tax_selic_monthly`); mês que falta deixa os juros e o total **sem valor**,
  com o mês dito, em vez de somar uma taxa pela metade.
- A tarefa `tax_selic` roda a cada 6 horas e só vai à rede quando falta um mês
  fechado (uma resposta de menos de 1 KB, de janeiro de 2025 até hoje). Falha
  da fonte não derruba nada: o que está guardado continua valendo.
- A calculadora fica na linha do mês, em DARF vencido e não marcado como pago
  ("Multa e juros"): a
  pessoa escolhe o dia do pagamento e vê a conta e o que vai em cada campo do
  DARF (código, período de apuração, vencimento, principal, multa e juros). É
  estimativa; o Sicalc da Receita é quem emite.

## 3. O que o Prumo NÃO calcula

- Opções, termo, futuro, swap e mercado de balcão fora de bolsa.
- Exercício de opções. A venda por exercício não tem a isenção de R$ 20 mil, e
  o Prumo não sabe que uma venda veio de exercício.
- Aluguel de ações (BTC), tanto a remuneração do doador quanto a operação do
  tomador.
- Ouro ativo financeiro.
- ETF de renda fixa (outra regra, Lei 13.043/2014), fundos fora de bolsa,
  clubes de investimento e come-cotas.
- Ações fora de bolsa (ganho de capital) e investimentos no exterior (Lei
  14.754/2023): ações e REITs nos EUA, ETFs no exterior e cripto em exchange
  estrangeira. Fica só o BDR, que é negociado na B3.
- Cripto:
  - permuta de cripto por cripto (é alienação, mas o livro só tem compra e
    venda em reais);
  - alienações no exterior somando no limite de R$ 35 mil;
  - a regra de somar alienações parciais do mesmo bem até o fim do ano seguinte
    para escolher a faixa (P&R 574). O Prumo avisa quando o ganho do mês passa
    de R$ 5 milhões.
- Eventos societários além de desdobramento, grupamento e bonificação:
  incorporação, cisão, troca de ticker, amortização e devolução de capital,
  subscrição.
- A retenção na fonte da nota: sem o valor informado, o Prumo usa o que a
  regra manda reter, como se fosse uma corretora só (§ 2.7). Não confere se a
  corretora reteve certo.
- A corretora de cada negócio: o day trade é casado como se tudo fosse numa
  instituição só (§ 7.2).
- As condições de isenção do rendimento de FII e Fiagro (100 cotistas, menos de
  10% das cotas): o Prumo supõe que valem.
- IRPF mínimo (Lei 15.270/2025, art. 16-A) e a retenção de dividendos acima de
  R$ 50 mil: o Prumo guarda a regra, mas não apura o mínimo.
- O DARF pago de verdade: a marca de "pago" é da pessoa, na tela (§ 8), e não
  muda a conta. A multa e os juros (§ 2.11) são calculados para o dia que a
  pessoa escolhe, não lidos do DARF pago.
- Declaração em conjunto, espólio e não residente.
- CNPJ da fonte pagadora fora do cadastro da CVM (§ 2.10): a linha sai pelo
  ticker, e a pessoa confere no informe de rendimentos.

## 4. Arredondamento

- Valores intermediários com precisão total.
- O resultado de cada grupo, o imposto, o IR retido e o DARF são arredondados
  a centavos (meio centavo para cima).
- A Receita não publica a regra de arredondamento por linha do demonstrativo.
  Diferença de centavos contra o programa IRPF é esperada.

## 5. Dúvidas que pedem confirmação humana (contador ou Receita)

1. **Day trade no limite de R$ 20 mil.** O art. 59, I, da IN 1.585 fala no
   "total das alienações desse ativo" no mercado à vista, e o art. 65, § 15,
   diz só que o *ganho* de day trade não é isento. O Prumo **soma** as vendas
   em day trade no total, a leitura conservadora: pode tirar a isenção de um
   mês que outra leitura deixaria isento.
2. **FII e Fiagro, mesma espécie?** O art. 37, § 2º, da IN 1.585 limita a
   compensação a "fundo da mesma espécie". O Prumo separa FII de Fiagro, também
   a leitura conservadora. Falta confirmar como o programa IRPF trata Fiagro no
   demonstrativo de FII.
3. **Códigos de Bens e Direitos de ações, FII, Fiagro, ETF e BDR.** Os
   códigos (03/01, 07/03, 07/02, 07/09, 04/04) e os de rendimentos (09, 20, 05,
   99, 10) não aparecem na P&R 2026. Vieram de informes de corretoras: conferir
   no programa IRPF 2026 e, para 2026, no IRPF 2027.
4. **Cripto: ganho por alienação ou por mês?** O Prumo soma os lucros por
   alienação do mês e ignora os prejuízos (P&R 583). Confirmar se o lucro e o
   prejuízo do mesmo criptoativo no mesmo mês podem se compensar.
5. **31 de dezembro.** A Agenda Tributária de 2025 venceu o DARF em 30/12. O
   Prumo trata todo 31/12 como dia sem expediente. Conferir a Agenda de dezembro
   de 2026 quando sair. Carnaval e Corpus Christi também ficam fechados; nenhum
   dos dois cai no último dia útil de um mês entre 2025 e 2027.
6. **Sobra de IR retido no fim do ano.** O Prumo não leva a sobra para janeiro
   (P&R 715 para day trade e, por analogia, o § 7º do art. 2º da Lei 11.033
   para o 0,005%). Confirmar onde informar a sobra do 0,005% na declaração.
7. **Dividendos com retenção em 2026.** Confirmar em que ficha do IRPF 2027
   entram o dividendo com retenção de 10% e o imposto retido, que é antecipação
   do IRPF mínimo.
8. **JCP pelo valor líquido.** O Prumo leva o valor líquido para a linha 10.
   Confirmar no programa.
9. **Arredondamento.** Arredondar por grupo e por mês (§ 4) contra o que o
   programa IRPF faz.
10. **P&R 706** remete à "pergunta 682" para o IR retido no day trade. Parece
    um erro de numeração: o tema está na 712.
11. **Base do 1% do day trade.** O art. 65, § 1º, II, da IN 1.585 fala no
    resultado positivo "por dia, ativo e instituição"; o § 4º e a P&R 705
    admitem compensar as perdas de day trade do mesmo dia. O Prumo calcula o 1%
    sobre o resultado do dia com as perdas de todos os ativos compensadas.
    Confirmar com a nota da corretora: o mês com IR retido pela regra já leva o
    aviso "confira na nota", e o valor da nota, quando informado, vale no lugar.

## 6. Como ler a saída

- `monthly::assess(&TaxInput, até_o_ano)` devolve um `MonthReport` por mês,
  com estas partes:
  - `sales`: vendas e resultados por instrumento, comum e day trade;
  - `exemptions`: o isentômetro, com vendas, limite, quanto falta, se o mês é
    isento e o lucro isento;
  - `pools`: resultado, prejuízo antes, usado e depois, base, alíquota e
    imposto de cada grupo;
  - `capital_gain`: a cripto;
  - `withholding`: o IR retido, com as bases e o valor da regra (0,005% e
    1%), o informado (se houver), o que abateu e o que sobra;
  - `darfs`: código, imposto, acumulado, se paga, valor, vencimento e o que
    fica para o mês seguinte;
  - `notices`: avisos.
- `irpf::annual_report(&TaxInput, proventos, ano)` devolve:
  - `assets`: Bens e Direitos;
  - `exempt_income` e `exclusive_income`: os rendimentos;
  - os meses do ano;
  - o prejuízo e o DARF pendente em 31/12;
  - a sobra de IR retido.
- Cada linha de rendimento traz `by_payer`: os tickers, o CNPJ e o nome da
  fonte pagadora (`Payer`, quando o Prumo achou na CVM), o bruto, o retido e o
  líquido, e os meses de dividendo acima do limite da retenção de 2026.
- `late::late_charge(regra, principal, vencimento, pagamento, selic)` devolve
  os dias de atraso, a multa (taxa e valor), os meses de Selic somados, o 1% do
  mês do pagamento, os juros e o total (sem valor quando falta a Selic de algum
  mês).

## 7. Ligação com o livro

A ponte mora em `backend/src/domain/tax_book.rs` (pura, com teste) e é lida
por `http/tax.rs`. Ela monta o `TaxInput` a partir do livro de lançamentos
(`transactions`) e dos proventos (`income_events`), sempre em reais.

### 7.1 Instrumento fiscal de cada ativo

O IR não vê a carteira como a carteira se vê. Cada ativo tem
`assets.tax_instrument` (migração 0015):

| Instrumento | Quando é o padrão |
|---|---|
| `stock` | Ações Nacionais que não são ETF nem BDR |
| `etf` | ETF de renda variável da B3: fundo de índice pelo nome ("fundo de índice", "ETF"), o ETF de bitcoin (Cripto negociado na B3), o de S&P 500 (Ações Internacionais na B3), o de IFIX |
| `bdr` | código com 31 a 35 ou 39 no fim (`AAPL34`) ou DRN/DR1/DR2/DR3/DRE no nome |
| `real_estate_fund` | FIIs |
| `agribusiness_fund` | FIIs com "agro" no nome |
| `crypto` | Cripto fora da B3 |
| `unsupported` | Renda Fixa e Renda Fixa Internacional (inclusive Tesouro e ETF de IMA-B), ETF de renda fixa em outra classe, Ações Internacionais e REITs fora da B3 |

- O padrão sai de `TaxInstrument::default_for` (classe, `listed_on_b3` e as
  mesmas pistas de ETF e BDR que `quotes::classify_b3` usa). A migração 0015
  faz a mesma conta em SQL para os ativos que já existiam, e o teste dela
  confere as duas com uma carteira fictícia.
- A pessoa troca na folha do ativo ("No imposto de renda"), com a explicação
  de cada escolha. Sem escolha (`tax_instrument_custom = false`), o
  instrumento acompanha o padrão quando a marca da B3 ou o nome mudam.
- `unsupported` tira o ativo inteiro da conta e o lista em "não calculado".
- Lançamento de ativo já removido usa o instrumento do mesmo ticker que
  ainda está na carteira, ou o padrão da foto que o lançamento guardou.

### 7.2 Do lançamento à operação do IR

O preço médio da Receita é por ticker: os lançamentos se juntam pelo
símbolo. Cada ticker é convertido na ordem do livro.

| Lançamento | No IR |
|---|---|
| `opening` no começo do livro | posição de partida (`OpeningPosition`), com o custo do saldo inicial |
| `opening` no meio do livro, com posição | bonificação com o custo informado (entram cotas com custo, sem negócio) |
| `opening` depois de a posição zerar | compra |
| `opening` sem preço | o ticker sai: sem custo não há preço médio |
| `buy` / `sell` | compra e venda, com preço e taxas × câmbio do lançamento |
| `split` / `reverse_split` | fator = (posição ± cotas) ÷ posição |
| `bonus` | bonificação com o custo atribuído (zero sem preço) |
| `adjustment` | muda a quantidade com o custo total igual, como no livro, e vira aviso |
| `adjustment` que zera a posição | o ticker sai (se foi venda, falta o preço) |
| `amortization` | o lançamento sai e vira aviso: o custo fica maior que o real |
| `redemption` | o ticker sai (é regra de renda fixa) |

- **Desdobramento e grupamento.** A B3 informa as cotas recebidas ou
  perdidas, e o livro guarda assim; o domínio do IR quer o fator. A conversão
  usa a posição do próprio IR naquele dia: o IR aplica os eventos do dia antes
  dos negócios (`positions::replay`), então o fator sai sobre a posição antes
  das compras e vendas do dia, e a quantidade no fim do dia fecha com a do
  livro. Um grupamento de 3 para 1 tem fator 1/3, que não cabe em decimal: o
  domínio arredonda a quantidade a 10 casas depois do fator (a precisão do
  livro), e 99 cotas viram 33, não 32,999….
- **Dólar.** Preço e taxa de cada lançamento vão a reais pelo `fx_rate`
  gravado nele (o câmbio do dia do negócio).
- **Corretora e IR retido.** O livro não guarda a corretora nem a nota: o day
  trade é casado como se fosse uma corretora só, e o IR retido do mês sai pela
  regra (§ 2.7), com o aviso "confira na nota". A pessoa informa o retido da
  nota por mês, e ele vale no lugar da regra.
- Um evento que não fecha com a posição do dia (desdobramento sem posição,
  venda maior que a posição pela ordem do IR) tira o ticker da conta, com o
  motivo.

### 7.3 Proventos

Entram os recebidos até hoje; estimativa (`source = estimate`) e
provisionado (pagamento no futuro) não entram.

| Provento | Ativo | No IRPF |
|---|---|---|
| dividendo | ação (ou ativo removido) | isento, `09` |
| JCP | ação (ou ativo removido) | tributação exclusiva, `10` |
| rendimento ou dividendo | FII ou Fiagro | isento, `99` |
| rendimento | ativo removido | isento, `99` |
| qualquer | BDR | fora: rendimento do exterior |
| qualquer | ETF, cripto, fora do IR | fora |
| amortização, outro | qualquer | fora |

O JCP retido na fonte é **15% até 2025 e 17,5% desde 2026**, pela data do
pagamento (`domain::income::jcp_withholding_rate`, a mesma alíquota da tabela
do IR, conferida em teste).

### 7.4 Saldo de antes do Prumo e DARF pago

- `tax_carryforward` (0015): o prejuízo a compensar por grupo e o imposto
  abaixo do mínimo ainda não pago, como estavam em 1º de janeiro do primeiro
  ano apurado (`year`). Sem linha, a conta começa do zero no primeiro ano com
  regra (2025). Com o saldo em 1/1/2026, pedir 2025 é erro
  (`tax_year_before_balance`).
- `tax_darf_payments` (0015): a marca de "paguei" por ano, mês e código, com o
  valor de quando a pessoa marcou. Se o livro muda depois, a tela avisa que o
  DARF de agora é outro valor. A marca não muda a conta.
- `tax_withholding_informed` (0029): o IR retido do mês que a pessoa leu na
  nota (ano, mês, valor). Sem linha, vale a regra.
- `tax_selic_monthly` (0029): a Selic de cada mês fechado (SGS 4390), dado
  público compartilhado entre as carteiras, para os juros do DARF em atraso.

## 8. A tela Impostos

Ajustes › Impostos (`#/ajustes/impostos`). Desenho em `docs/design.md` § 10.

- O **aviso de responsabilidade** do topo desta página, sempre à vista.
- O **isentômetro** do mês de hoje: vendas de ações contra o limite, com a
  barra, e quanto ainda cabe.
- Os **meses do ano**: vendas de ações, imposto, DARF (código e valor),
  vencimento e a marca de pago; abaixo do mínimo, "soma no mês seguinte". DARF
  vencido e não pago fica "Em atraso", com "Multa e juros" (§ 2.11). Cada mês
  com movimento abre a apuração por grupo (resultado, prejuízo antes,
  compensado, base, alíquota, imposto, prejuízo que fica), a cripto e o IR
  retido (pela regra ou da nota, com "Informar o valor da nota").
- O **prejuízo a compensar** de cada grupo, mês a mês, numa coluna da mesma
  tabela (§ 2.6).
- A **declaração do ano**: Bens e Direitos com o custo em 31/12 e os
  rendimentos isentos e de tributação exclusiva numa tabela só, uma linha por
  fonte pagadora com ficha, código, CNPJ e nome (§ 2.10), cada linha com
  "Copiar" e o relatório inteiro em "Copiar o relatório".
- **O que não foi calculado**: o que saiu do livro e dos proventos, com o
  motivo.
- **O que o Prumo cobre**: cada regra do ano com o número e a fonte (da tabela
  do servidor) e o que fica fora (§ 3), com o porquê.
- As **dúvidas do § 5** aparecem como "Ponto a confirmar com contador" ao lado
  do número que elas mexem: o day trade no isentômetro (5.1), o prejuízo de
  Fiagro (5.2), os códigos da declaração (5.3), a cripto por venda (5.4), o
  DARF de novembro perto de 31/12 (5.5), a sobra de IR retido (5.6), o
  dividendo com retenção (5.7) e o JCP líquido (5.8).
- O **saldo de antes do Prumo**, para informar e editar.
- O olho fechado mascara todo R$ e toda quantidade (percentuais e datas
  ficam). O "Copiar" copia o valor de verdade: é o que a pessoa pediu.
