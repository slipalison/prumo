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

- Fontes consultadas em **25/11/2026**.
- "P&R" é o **Perguntas e Respostas IRPF 2026** da Receita (v1.00, de
  23/04/2026; ano-calendário 2025), em
  [gov.br/receitafederal › Perguntas e respostas › DIRPF](https://www.gov.br/receitafederal/pt-br/centrais-de-conteudo/publicacoes/perguntas-e-respostas/dirpf/p-r-irpf-2026-v1-00-2026-04-23.pdf/view).
- Leis no [planalto.gov.br](https://www.planalto.gov.br/); a IN RFB
  1.585/2015 em [normas.receita.fazenda.gov.br](http://normas.receita.fazenda.gov.br/sijut2consulta/link.action?idAto=67494).

## 1. Como a regra fica no código

- Cada ano-calendário é uma função em `domain/tax/rules.rs` (`year_2025`,
  `year_2026`). Cada alíquota, limite, código de DARF e código da declaração
  vem com a fonte ao lado (`Sourced { value, source }`), e a tela pode
  mostrar a fonte.
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
- O Prumo identifica o day trade sozinho, pelo dia, ativo e instituição do
  lançamento. FII e Fiagro também têm day trade (o casamento tira a compra do
  preço médio), mas o resultado fica no grupo do próprio fundo, que já é 20%.

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

### 2.7 IR retido na fonte ("dedo-duro")

- Operação comum em bolsa: retenção de **0,005%** sobre o valor da venda,
  dispensada quando dá R$ 1,00 ou menos (ou seja, até R$ 20 mil vendidos).
  Fonte: Lei 11.033/2004, art. 2º, §§ 1º e 4º; P&R 706.
- Day trade: **1%** sobre o resultado (P&R 712).
- O retido **abate o imposto do mês** em bolsa (código 6015). O que sobra passa
  para os meses seguintes **só dentro do ano-calendário**. A sobra de dezembro
  não vai para o ano seguinte: a do 0,005% compensa na declaração anual, e a
  do day trade pode ser pedida em restituição. Fonte: Lei 11.033/2004, art. 2º,
  § 7º; IN RFB 1.585/2015, art. 65, §§ 8º e 9º; P&R 714 e 715.
- O Prumo recebe o retido que veio na nota (não calcula a retenção) e mostra a
  sobra de dezembro no fechamento do ano.

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
- O Prumo guarda a regra (limite e alíquota) e soma o retido informado. **Não
  calcula o IRPF mínimo.**

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
- A retenção na fonte: o Prumo usa o IR retido informado na nota e não confere
  se a corretora reteve certo.
- As condições de isenção do rendimento de FII e Fiagro (100 cotistas, menos de
  10% das cotas): o Prumo supõe que valem.
- IRPF mínimo (Lei 15.270/2025, art. 16-A) e a retenção de dividendos acima de
  R$ 50 mil: o Prumo guarda a regra, mas não apura o mínimo.
- Multa e juros de DARF pago em atraso. Também não sabe se um DARF foi pago:
  isso é da tela, na onda 2.
- Declaração em conjunto, espólio e não residente.
- CNPJ da fonte pagadora: a declaração pede o CNPJ, e o Prumo só tem o ticker.

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

## 6. Como ler a saída (para a onda 2)

- `monthly::assess(&TaxInput, até_o_ano)` devolve um `MonthReport` por mês,
  com estas partes:
  - `sales`: vendas e resultados por instrumento, comum e day trade;
  - `exemptions`: o isentômetro, com vendas, limite, quanto falta, se o mês é
    isento e o lucro isento;
  - `pools`: resultado, prejuízo antes, usado e depois, base, alíquota e
    imposto de cada grupo;
  - `capital_gain`: a cripto;
  - `withholding`: o IR retido;
  - `darfs`: código, imposto, acumulado, se paga, valor, vencimento e o que
    fica para o mês seguinte;
  - `notices`: avisos.
- `irpf::annual_report(&TaxInput, proventos, ano)` devolve:
  - `assets`: Bens e Direitos;
  - `exempt_income` e `exclusive_income`: os rendimentos;
  - os meses do ano;
  - o prejuízo e o DARF pendente em 31/12;
  - a sobra de IR retido.
