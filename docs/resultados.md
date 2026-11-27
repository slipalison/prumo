# Resultados trimestrais

O que cada empresa da carteira entregou à CVM no último trimestre, contra o
mesmo trimestre do ano anterior e contra o trimestre anterior, e os FIIs mês a
mês pelo informe mensal. Tela em `#/analise/resultados` (link no cabeçalho da
Análise); rota `GET /api/v1/results` ([api.md](api.md#resultados)).

**Informa, não recomenda.** Nenhum "comprar", "barato" ou ranking: a tela
mostra o que as companhias entregaram. As cores (verde para cima, vermelho para
baixo) só reforçam o texto, que diz tudo sozinho.

O código: regra pura em `backend/src/domain/fundamentals/quarters.rs` (os
trimestres e a variação) e `fund.rs` (`fund_months`, a série do FII); a leitura
em `backend/src/store/results.rs`; a rota em `backend/src/http/results.rs`; a
tela em `frontend/src/features/results/`. A ingestão é a dos fundamentos
([fundamentos.md](fundamentos.md)): nenhuma fonte nova, nenhum host novo.

## De onde sai cada trimestre

O ITR traz o resultado duas vezes, o do trimestre e o acumulado do ano. O
Prumo guarda o acumulado (é com ele que monta os 12 meses dos fundamentos) e
tira o trimestre da diferença entre acumulados do mesmo exercício — a mesma
conta que a companhia faz:

| Trimestre | Conta | `method` |
|---|---|---|
| 1º | o acumulado de 3 meses do 1º ITR (é o próprio trimestre) | `first_quarter` |
| 2º | acumulado de 6 meses − acumulado de 3 meses | `year_to_date_difference` |
| 3º | acumulado de 9 meses − acumulado de 6 meses | `year_to_date_difference` |
| 4º | **o exercício da DFP − o acumulado de 9 meses do 3º ITR** (não existe ITR do 4º trimestre) | `annual_less_nine_months` |

- **Qual acumulado vale.** O de cada período vem da coluna do documento que o
  fecha (o ITR do trimestre, a DFP do ano) e, sem ele, da coluna de comparação
  do documento do ano seguinte (o ITR de setembro de 2026 traz o acumulado de
  setembro de 2025). A mesma preferência da série anual; entre iguais, o
  documento mais novo e a versão mais alta.
- **Sem um dos acumulados, não há o trimestre.** A DFP sozinha não diz o 4º
  trimestre; um ITR que falta tira o trimestre dele e o seguinte. Nada é
  estimado.
- **Os arquivos lidos** são os dos fundamentos: ITR do ano corrente e do
  anterior e as DFP dos seis anos. Com as colunas de comparação, isso cobre o
  1º trimestre de dois anos atrás em diante: os **oito trimestres** da série e
  o trimestre de um ano antes do mais novo.
- **Exercício fora do ano civil** funciona: o trimestre é contado dentro do
  exercício (`fiscal_quarter`), e a tela escreve os meses ("jul–set/2026")
  quando o exercício não é o ano civil.

**Conferido contra o CSV cru.** O teste
`the_quarters_match_the_quarter_rows_of_the_raw_files` (em
`backend/src/fundamentals/cvm.rs`) monta ITR e DFP fictícios no formato da CVM,
com as linhas do trimestre **e** as do acumulado, lê pelo leitor do Prumo (que
descarta as do trimestre) e confere que cada trimestre montado é igual à linha
do trimestre do arquivo — para uma companhia que divide o lucro e para outra
que entrega a divisão toda em zero.

### As linhas

| Medida | Conta |
|---|---|
| Linha de cima (`top_line`) | plano comercial: receita líquida (DRE 3.01); banco: **resultado bruto da intermediação financeira** (DRE 3.03); seguradora: **resultado bruto** das operações de seguro (DRE 3.03) |
| Lucro líquido | o mesmo dos fundamentos: a parte dos **controladores**, achada pela descrição; divisão entregue toda em zero com total diferente de zero vale como não preenchida (o total conta) |
| Margem líquida | lucro ÷ receita, só no plano comercial e fora de holding |

**Banco e seguradora.** A "receita" deles (3.01: receitas da intermediação,
receitas das atividades seguradoras) é bruta e não se compara com a de
indústria; o que sobra da atividade depois do custo dela é a 3.03, e é ela a
linha de cima. A margem vem com `gap: "financial_template"`.

**O lucro na mesma base nas duas pontas.** O trimestre usa a parte dos
controladores quando os dois acumulados dividem o lucro de verdade; se um deles
entrega a divisão toda em zero, as duas pontas usam o total. Misturar (total
menos controladores) daria um trimestre que nenhuma demonstração tem.

**Plano trocado entre os acumulados** (raríssimo): a linha de cima do
trimestre fica `not_reported`, porque a conta de antes não é a mesma.

## A variação

Do trimestre mais novo contra o mesmo trimestre do ano anterior (`year_ago`) e
contra o trimestre imediatamente anterior (`previous_quarter`):

| Antes → agora | `kind` | Na tela (lucro) | Na tela (linha de cima) |
|---|---|---|---|
| positivo → qualquer valor ≥ 0 | `percent` (`agora ÷ antes − 1`) | `+25,0%` / `−3,2%` | o mesmo |
| positivo → negativo | `entered_loss` | entrou no prejuízo | ficou negativo |
| negativo → positivo ou zero | `left_loss` | saiu do prejuízo | saiu do negativo |
| negativo → negativo menor | `smaller_loss` | prejuízo menor | negativo menor |
| negativo → negativo maior | `larger_loss` | prejuízo maior | negativo maior |
| igual (≠ 0) | `unchanged` | igual | igual |
| zero → qualquer | `no_base` | sem base | sem base |
| falta um dos números | `missing` (com `gap`) | — | — |

**Nunca percentual sobre base negativa.** Prejuízo que diminui sairia como
"queda", e lucro depois de prejuízo, como "−300%": é o defeito que os sites de
cotação mostram. A margem compara em **pontos** (`net_margin_points`, fração:
`0,012` = +1,2 p.p.).

## As bolhas

Lucro × linha de cima do trimestre mais novo: x = variação do lucro, y =
variação da receita (em banco e seguradora, do resultado bruto), tamanho da
bolha = **peso na carteira** (`weight`, fração do valor total, do servidor).
Alterna entre "contra o ano anterior" e "contra o trimestre anterior".

- Só entra quem tem as **duas** variações em percentual. Prejuízo numa ponta
  não vira número no eixo: a empresa vai para a linha "Fora do gráfico", com o
  rótulo ("lucro: saiu do prejuízo").
- O eixo sempre tem o zero e vai no máximo de −200% a +200%; quem passa fica na
  borda com o número escrito ao lado do nome ("VARE3 +500,0%").
- Não é ranking: a posição diz o que aconteceu, não o que fazer. Uma tabela
  escondida traz os mesmos números para o leitor de tela.

## FIIs mês a mês

O pedido era: se o informe mensal der **rendimento distribuído** e **resultado
por cota**, mostrar a série mensal; se não, dizer por quê. Conferido no arquivo
de 2026 (`inf_mensal_fii_complemento`):

- **Não há rendimento em reais nem resultado por cota.** O informe mensal traz
  o patrimônio, as cotas e três percentuais: `Percentual_Dividend_Yield_Mes`
  (o rendimento do mês **sobre o valor patrimonial da cota**),
  `Percentual_Rentabilidade_Patrimonial_Mes` e a efetiva (a soma das duas).
  Nos fundos conferidos, o rendimento do mês ÷ o valor patrimonial da cota do
  fim do mês anterior bate com o percentual; mas a conta é de cada
  administrador, e o Prumo não deriva reais dela.
- **O resultado do fundo** só sai no informe **trimestral**
  (`inf_trimestral_fii_resultado_contabil_financeiro`), somado por semestre
  (`Resultado_Financeiro_Liquido_Acumulado` volta a zero no 3º trimestre): não é
  série mensal.
- **O rendimento em reais por cota** a pessoa já tem na Renda, pelos avisos da
  B3 (fonte melhor que um percentual).

Então a tela mostra o que o informe mensal tem, como o fundo informou: o
**rendimento do mês sobre o valor patrimonial** e o **valor patrimonial por
cota** (patrimônio ÷ cotas), dos 12 meses mais novos, e diz no ⓘ o que falta e
onde está. Rendimento **negativo ou acima de 10% no mês** é erro de
preenchimento (há fundo que escreve `0,89` querendo dizer 0,89%) e vem com
`gap: "implausible"`, nunca como número.

A série é gravada na leitura do informe mensal (tabela `cvm_fund_months`,
migração 0031), só dos fundos com cota negociável (ISIN de cota), a versão
maior de cada mês.

## "Não se aplica": holding, banco e seguradora

Onde o Prumo mostra indicador de empresa (esta tela, a folha do ativo, o
comparador e a Análise), o indicador que enganaria diz **"não se aplica"**, com
a dica do porquê, em vez de um número. **Uma regra só**: os gaps saem de
`company_metrics` (o plano de contas e `CompanyMetrics::holding`), e as quatro
telas leem os mesmos `Figure`; no comparador, a célula sem número fica fora do
destaque de maior/menor.

| Quem | O que não se aplica | Por quê |
|---|---|---|
| Banco | receita, margens, dívida, EBITDA, caixa operacional, CAGR da receita | o plano de contas da CVM para instituição financeira não tem as contas (`financial_template`) |
| Seguradora | o mesmo | idem |
| Holding de participações | margem líquida, margem EBITDA, dívida líquida ÷ EBITDA, CAGR da receita | o lucro vem da equivalência patrimonial e não passa pela receita (`holding`): a margem passaria de 100% sem dizer nada |

**Quem é holding.** O cadastro da CVM põe a companhia no setor de
administração de participações (`SETOR_ATIV` começando por "Emp. Adm.
Part.") **e** a equivalência patrimonial dos últimos 12 meses é pelo menos
**metade** do lucro (em módulo; prejuízo também conta). O cadastro sozinho
não basta: medido nas DFP de 2025, a WEG, a CPFL, a Neoenergia, a BRF e
dezenas de outros grupos estão no setor de participações e consolidam as
controladas — a receita é a do grupo e a margem vale (equivalência abaixo de
0,3 vez o lucro). As holdings puras (Itaúsa, BB Seguridade, Bradespar, Caixa
Seguridade...) ficam entre 0,76 e 2,3 vezes. A equivalência é a conta de
resultado com "equivalência patrimonial" na descrição (DRE 3.04.06 no plano
comercial, 3.06 no das seguradoras), que a migração 0031 passou a guardar.

O que vale para holding continua com número: receita, lucro, ROE, dívida
líquida ÷ patrimônio, LPA, VPA e o CAGR do lucro. Holding no plano de
seguradora (a BB Seguridade) mantém o motivo `financial_template`.

Na **Análise**, as regras de dívida ÷ EBITDA e de margem EBITDA deixam banco,
seguradora e holding fora da conta, e a de caixa operacional, banco e
seguradora; o detalhe da regra diz em quem não se aplicou e por quê
(`not_applicable`, ver [analise.md](analise.md)). O ponto de atenção de margem
líquida baixa e o de crescimento de receita baixo não aparecem para holding.

## Data de entrega

`received_on` é a data em que a CVM recebeu o documento que fecha o trimestre
(o ITR dele, ou a DFP no 4º). Não é lida de novo aqui: vem da tabela que a
**Agenda** grava do índice dos arquivos (`cvm_filing_receipts`, `DT_RECEB` de
`itr_cia_aberta_AAAA.csv` e `dfp_cia_aberta_AAAA.csv`, ano corrente e anterior;
ver [agenda.md](agenda.md)), uma cópia só para as duas telas. Reapresentação
(versão maior) traz a data dela. Sem a entrega lida, ou com o trimestre montado
só da coluna de comparação de um documento posterior, `null`.

## Migração 0031

- A tabela `cvm_fund_months` (a série mensal dos FIIs).
- **Releitura única**: zera `scanned_codes` dos arquivos de DFP e ITR (a
  próxima rodada lê cada um de novo, inteiro, para trazer a conta de
  equivalência patrimonial que diz se a companhia é holding) e `checked_at` do
  informe mensal (traz a série). São os ~140 MB da primeira leitura, uma vez; o
  gravado continua valendo até lá.

## Limitações

- O trimestre depende dos ITR do ano corrente e do anterior: companhia que
  entrou na carteira depois tem a série a partir da próxima rodada.
- Sem estimativa de analistas, sem projeção, sem consenso: o Prumo não tem
  fonte pública disso e não recomenda.
- Lucro de holding e de banco é o da CVM; o "lucro recorrente" que as
  companhias divulgam em release não está nos dados abertos.
- FII: o percentual do informe mensal é o que o administrador calculou; o
  Prumo só descarta o impossível.
