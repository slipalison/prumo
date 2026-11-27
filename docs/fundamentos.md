# Fundamentos

Os indicadores fundamentalistas de cada empresa e de cada FII da carteira, de
fontes públicas e gratuitas: os **dados abertos da CVM** e, para ligar o
ticker à companhia e contar as ações, a **página pública do emissor na B3**.
Servem à folha do ativo (seção "Fundamentos") e à Análise da carteira
(dimensão Qualidade, pontos de atenção e o Graham), que os consome pelo
contrato do fim deste arquivo.

**Nada é inventado.** Toda medida é o número ou o motivo de não haver número
(`gap`). Conta que o plano do banco não tem, patrimônio negativo no
denominador, ano sem balanço: vira motivo, nunca zero.

O código: regra pura em `backend/src/domain/fundamentals.rs` (com
`fundamentals/company.rs` e `fundamentals/fund.rs`), leitura dos arquivos em
`backend/src/fundamentals/cvm.rs`, download em `fundamentals/download.rs`, a B3
em `fundamentals/b3.rs`, a tarefa em `fundamentals/mod.rs`, o banco em
`store/fundamentals.rs` (migrações 0020 e 0031) e a rota em `http/fundamentals.rs`.
Os resultados trimestrais e a série mensal dos FIIs, que saem dos mesmos
arquivos, estão em [resultados.md](resultados.md).

## Fontes

Conferidas em 26/11/2026 (todas respondem 200, sem login nem chave):

| Arquivo | Endereço | O que o Prumo usa |
|---|---|---|
| Cadastro das companhias | `dados.cvm.gov.br/dados/CIA_ABERTA/CAD/DADOS/cad_cia_aberta.csv` | código CVM, CNPJ, nome, `SIT`, `SIT_EMISSOR`, setor |
| DFP (anual) | `.../CIA_ABERTA/DOC/DFP/DADOS/dfp_cia_aberta_<ANO>.zip` | BPA, BPP, DRE, DFC (MD e MI), DVA; consolidada e individual |
| ITR (trimestral) | `.../CIA_ABERTA/DOC/ITR/DADOS/itr_cia_aberta_<ANO>.zip` | o mesmo, com o acumulado do ano |
| Informe mensal de FII | `.../FII/DOC/INF_MENSAL/DADOS/inf_mensal_fii_<ANO>.zip` | `geral` (ISIN, nome) e `complemento` (patrimônio líquido, cotas emitidas e o rendimento do mês sobre o valor patrimonial, de cada mês) |
| Informe trimestral de FII | `.../FII/DOC/INF_TRIMESTRAL/DADOS/inf_trimestral_fii_<ANO>.zip` | `geral` (o informe mais novo) e `imovel` (área e vacância de cada imóvel) |
| Emissor na B3 | `sistemaswebb3-listados.b3.com.br/listedCompaniesProxy/CompanyCall/GetListedSupplementCompany/<base64>` | `codeCVM` e `totalNumberShares` |

Formato dos CSV da CVM: `;` como separador, ISO-8859-1 (latin1), datas ISO,
ponto decimal. **Sem aspas como delimitador**: há descrição com aspas no meio
(`Garantia "Escrow"`, `"Baia Formosa", em zona urbana...`), e o leitor as trata
como texto. Valores com `ESCALA_MOEDA = MIL` são multiplicados por mil (o
banco guarda reais); linha em outra moeda ou escala é ignorada.

### Ticker → companhia, ticker → fundo

- **Companhia:** as quatro letras do ticker (`WEGE3` → `WEGE`) vão à página do
  emissor na B3, que devolve `codeCVM` (`5410`) — o `CD_CVM` dos arquivos da CVM
  (lá com zeros à esquerda, `005410`). ETF cadastrado em Ações Nacionais
  (`BOVA11`) volta com `codeCVM` `0`: fica sem fundamentos, com o motivo.
- **FII:** o ISIN da cota no informe mensal é `BR` + emissor + `CTF` + série
  (`BRKNRICTF007` → `KNRI`), a mesma regra da referência de setor. O emissor
  leva ao CNPJ do fundo, e o CNPJ liga o mensal ao trimestral. Quando mais de
  um CNPJ declara o mesmo emissor, a escolha é a de "Ticker de FII → fundo",
  logo abaixo.

### Ticker de FII → fundo

O informe da CVM não traz o código de negociação, e o ISIN que o fundo declara
pode estar errado: em 27/11/2026, 31 raízes de ISIN tinham mais de um CNPJ nos
informes de 2025 e 2026. No caso que apareceu na tela, um fundo novo preencheu
o ISIN de um fundo antigo e entregou um informe mais novo; o Prumo, que pegava
o informe mais novo do emissor, mostrava o valor patrimonial (R$ 0,75 em vez de
~R$ 97), o tipo e o "Pelas duas" do fundo errado, e o mesmo CNPJ errado ia para
o Fundos.NET da Agenda e para a fonte pagadora do informe de rendimentos do IR.

Agora há um caminho só, `Store::fund_cnpjs` (`store/fund_issuer.rs`), com a
regra em `domain::fund_issuer::choose_fund`, e tudo sai dele: fundamentos (VP,
P/VP), a série mensal dos Resultados (pelo CNPJ dos fundamentos), a referência
de setor (tipo e composição), o CNPJ do Fundos.NET e o da fonte pagadora.

1. **A B3, primeiro.** A página pública de fundos listados da B3 (a que o site
   dela usa, sem login nem chave) dá o CNPJ pelo código:
   `fundsListedProxy/Search/GetListFunds` com a sigla acha o fundo (`id`) e
   `GetDetailFund` devolve o `cnpj` e o `fundName`. A rodada dos fundamentos
   pergunta pelos FIIs das carteiras e guarda em `b3_fund_cnpj` (migração
   0036), com as mesmas idades do código CVM das companhias (achado: 20 horas;
   não achado: 7 dias) e a reserva por réplica. O que a B3 diz vale mesmo que
   nenhum informe tenha esse CNPJ: o fundo fica sem os números da CVM (com o
   motivo), nunca com os de outro; a fonte pagadora sai com o CNPJ e o nome da
   B3 (`payer_source: "b3_fund"`).
2. **Sem a B3** (fora do ar, ou a sigla não está na lista), entre os fundos em
   dia (último informe a até 3 meses do mais novo deles: o CNPJ antigo de um
   fundo que virou classe fica para trás), o que declara o emissor **há mais
   tempo** (primeiro mês na série guardada, que cobre os dois anos de informe
   que o Prumo lê); empate, o informe mais novo; depois, o menor CNPJ. Medido
   nos informes de 2025 e 2026 contra a B3: das 20 raízes disputadas em que a B3
   lista um fundo do informe, essa regra acertou 17 (o maior número de
   cotistas, 16; "negociado em bolsa" e depois cotistas, 15). As três que ela
   erra são fundos que declaram o mesmo ISIN desde o começo da série: só a B3
   separa. A rodada registra no log cada emissor disputado decidido sem a B3.

O que já estava gravado do fundo errado se corrige sozinho, sem depender de
migração: fundamentos, IR e Agenda são calculados na leitura; a referência de
setor guarda o CNPJ de onde cada linha saiu (`sector_reference.cnpj`) e é lida
de novo enquanto houver linha sem ele ou com um CNPJ diferente do que a B3
lista para a sigla (ver [`saude.md`](saude.md)).

### Número de ações (LPA e VPA)

`totalNumberShares` da B3 (ON + PN, em ações, **com** as em tesouraria). A
CVM tem a composição do capital (`*_composicao_capital_*.csv`), mas o arquivo
**não tem coluna de escala** e as companhias preenchem de jeitos diferentes:
medido em 26/11/2026, no mesmo arquivo uma companhia informa o total em ações
(4.197.317.998) e outra em milhares (11.026.869 para 11 bilhões de ações).
Usar a CVM daria LPA mil vezes errado sem aviso. A B3 publica em ações para
todas. As ações em tesouraria não são descontadas (a B3 não as publica e a CVM
as tem na mesma escala incerta); a diferença costuma ficar abaixo de 1%.

## Quando busca, e o quanto baixa

- **Uma tarefa em segundo plano**, que confere de hora em hora o que venceu.
  Cada arquivo é conferido **uma vez por dia** com GET condicional (`ETag` e
  `Last-Modified`): a CVM responde **304** quando nada mudou, e nada é baixado.
  O emissor na B3 é conferido uma vez por dia (o número de ações muda com
  desdobramento e grupamento); "a B3 não tem código" espera uma semana.
- **Só as companhias da carteira** (de todas as pessoas) saem do arquivo. Cada
  arquivo guarda os códigos CVM que a versão lida já cobriu
  (`cvm_sources.scanned_codes`). Companhia nova na carteira, numa versão já
  lida, obriga a ler o arquivo de novo **só para ela**; o resto fica como está.
- **Ativo novo dispara a busca:** cadastrar (ou importar da B3) uma ação ou FII
  acorda a tarefa, que roda 30 s depois (para juntar vários ativos de uma
  importação), sem esperar a hora.
- **Arquivos lidos:** cadastro; DFP do ano corrente e dos cinco anteriores
  (seis arquivos, ~13 MB cada); ITR do ano corrente e do anterior (~30 MB cada);
  informes de FII do ano corrente e do anterior (~1,5 MB e ~2 MB). Arquivo do
  ano que ainda não existe (janeiro) responde 404 e é conferido de novo no dia
  seguinte.
- **Em fluxo:** o corpo é lido enquanto chega, entrada a entrada do `.zip` e
  linha a linha do CSV, sem guardar o arquivo em memória nem em disco (o pod tem
  192 MiB; o ITR aberto passa de 700 MB). As demonstrações que o Prumo não usa
  (DMPL, DRA, parecer) nem são descompactadas. Medido com os arquivos reais de
  26/11/2026: as seis DFP e um ITR lidos para sete companhias em ~1,2 s
  (binário de release).
- **Fonte fora do ar não quebra nada:** a falha fica em
  `cvm_sources.last_error`, o gravado continua valendo e a próxima tentativa
  vem em 6 h. Arquivo que não é o esperado (sem a DRE, coluna renomeada, HTML
  de manutenção) é falha, não "zero companhias".
- **Duas réplicas, quatro conexões:** cada arquivo e cada emissor é reservado
  no banco (`claimed_until`; quem marca primeiro lê, a outra réplica pula) e
  cada gravação é uma transação curta com `pg_advisory_xact_lock`. Nenhuma
  conexão fica presa durante o download. A reserva devolve o estado do
  arquivo naquele instante, e é com ele que a réplica decide se lê: a lista
  lida no começo da rodada pode estar velha, porque a outra réplica pode ter
  acabado de ler o arquivo e soltado a reserva.
- `FUNDAMENTALS_ENABLED=false` desliga as buscas; o gravado continua servido.
- **A Agenda lê outros arquivos da CVM** com as mesmas peças (GET condicional,
  `cvm_sources`, reserva, `due_with`): o IPE do ano (fato relevante,
  comunicados) e só o índice do ITR e da DFP (a data de entrega), que é a
  primeira entrada do `.zip` — o download para ali. Ver [agenda.md](agenda.md).

## O que é guardado

Só as contas que a conta usa (`domain::fundamentals::keeps_line`), não o
balanço inteiro: na DRE, as contas de nível 1 e as de nível 2 com
"controlador" ou "equivalência patrimonial" na descrição (3.04.06, que diz se
a companhia é holding); no BPP, as de nível 1, a 2.01.04, a 2.02.01 e as
de nível 2 com "controlador"; no BPA, 1.01.01 e 1.01.02; na DFC, 6.01; na DVA,
7.04.01. Por documento (DFP ou ITR de uma data), a versão mais alta. (A data
de entrega de cada documento é da Agenda: `cvm_filing_receipts`.)

- **Reapresentação:** `VERSAO` maior vence; um arquivo velho com versão menor
  nunca desfaz a reapresentação.
- **Consolidada ou individual:** a consolidada, quando a companhia entrega;
  senão, a individual (companhia sem controladas só entrega a individual).
- **ITR:** o resultado vem duas vezes (o trimestre e o acumulado do ano); fica
  o acumulado.

## Contas e fórmulas

### Plano de contas

A conta 3.01 diz o plano:

| 3.01 | Plano |
|---|---|
| "Receita de Venda de Bens e/ou Serviços" | comercial (industrial, serviços, holding) |
| "Receitas da/de Intermediação Financeira" | banco |
| "Receitas das Atividades Seguradoras/Resseguradoras", "Receitas das Operações" (o plano antigo, até 2022) | seguradora |
| outra | desconhecido |

Em 2021–2025 só existiram esses cinco textos na conta 3.01 das DFP.

### Por período (exercício, ou últimos 12 meses)

| Medida | Conta da CVM | Banco / seguradora |
|---|---|---|
| Receita líquida | DRE 3.01 | não se aplica |
| EBIT | DRE 3.05 ("Resultado Antes do Resultado Financeiro e dos Tributos") | não se aplica |
| Lucro líquido | DRE "Lucro/Prejuízo ... do Período" (3.11 no comercial, 3.09 ou 3.11 nos bancos, 3.13 nas seguradoras), **a parte atribuída aos controladores** (3.11.01) quando há; divisão entregue toda em zero com total diferente de zero (há companhia sem minoritários que faz isso) vale como não preenchida, e conta o total | sim |
| Patrimônio líquido | BPP "Patrimônio Líquido" (2.03 no comercial, 2.07 ou 2.08 nos bancos), total | sim |
| Patrimônio dos controladores | a conta-filha "atribuído ao controlador" (bancos) ou o total menos "não controladores" (2.03.09) | sim |
| Dívida bruta | BPP 2.01.04 + 2.02.01 (empréstimos e financiamentos, curto e longo prazo; no plano da CVM incluem debêntures e arrendamento) | não se aplica |
| Caixa e aplicações | BPA 1.01.01 + 1.01.02 (caixa e equivalentes, aplicações financeiras de curto prazo) | não se aplica |
| Dívida líquida | dívida bruta − caixa e aplicações (negativa = caixa líquido) | não se aplica |
| D&A | DVA 7.04.01 ("Depreciação, Amortização e Exaustão"), em módulo | não se aplica |
| EBITDA | EBIT + D&A | não se aplica |
| Caixa operacional | DFC 6.01 | não se aplica |

O lucro e o patrimônio são achados pela **descrição** da conta de nível 1,
porque o código muda de plano para plano. As contas fixas (3.01, 3.05,
2.01.04...) só valem se a descrição for a esperada ("emprestimo",
"deprecia"...): o mesmo código com outro conteúdo é "a conta não veio".

**D&A vem da DVA, não da DFC.** Na DFC a depreciação é uma linha livre
(`6.01.01.xx`, `ST_CONTA_FIXA = N`), com nome e posição que cada companhia
escolhe; na DVA é a conta fixa 7.04.01, obrigatória para companhia aberta. Nas
amostras conferidas os dois números batem.

**O EBITDA é uma aproximação:** EBIT + D&A pela DVA. Não desconta itens não
recorrentes nem segue a Instrução CVM 527 (que pede o EBITDA da própria
companhia com reconciliação); o da companhia pode ser diferente.

| Razão | Fórmula | Sem número quando |
|---|---|---|
| ROE | lucro dos controladores ÷ patrimônio dos controladores **no fim do período** | patrimônio ≤ 0 |
| Margem líquida | lucro dos controladores ÷ receita | receita ≤ 0; holding (`holding`) |
| Margem EBITDA | EBITDA ÷ receita | receita ≤ 0; holding |
| Dívida líquida / EBITDA | dívida líquida ÷ EBITDA | EBITDA ≤ 0; holding |
| Dívida líquida / PL | dívida líquida ÷ patrimônio total | patrimônio ≤ 0 |

**ROE pelo patrimônio final, não pelo médio:** é o mesmo para o exercício e
para os 12 meses (no ITR, o balanço de 12 meses antes estaria noutro arquivo),
e é o que os sites de cotação costumam mostrar. Com o médio, o ROE de quem
aumentou o patrimônio no ano sairia maior. A dívida líquida é do grupo e divide
pelo patrimônio total (com os não controladores).

Razões com seis casas decimais; valores em reais, sem arredondar.

### Os últimos 12 meses

Se há ITR depois do último exercício, e os períodos se encaixam (o ano do ITR
começa no dia seguinte ao fim do exercício, e o acumulado de comparação do ITR
começa no mesmo dia que o exercício):

`12 meses = exercício + acumulado do ano − acumulado do ano anterior`

para receita, EBIT, D&A, lucro e caixa operacional; o balanço (patrimônio,
dívida, caixa) é o do ITR. Sem ITR posterior (ou sem encaixe), os 12 meses são
o próprio exercício. As datas (`period_start`, `period_end`) e as fontes
(`sources`) dizem sempre qual.

### A série de cinco exercícios

O último exercício entregue e os quatro de antes. Cada exercício vem da DFP
dele (a coluna `ÚLTIMO`); sem ela, da coluna de comparação (`PENÚLTIMO`) da DFP
do ano seguinte — que já traz a reapresentação. Sem nenhuma das duas, o ano
fica vazio (`figures: null`).

| Medida | Regra |
|---|---|
| Anos com lucro | exercícios com lucro > 0, sobre os exercícios com o lucro conhecido |
| Anos com caixa operacional positivo | o mesmo com a DFC 6.01 |
| CAGR de receita e de lucro | `(último ÷ primeiro)^(1/4) − 1`, entre as pontas da janela (cinco exercícios, quatro passos). Sem uma das pontas: sem CAGR (`insufficient_history`); com prejuízo (ou receita ≤ 0) numa ponta: sem CAGR (`non_positive_endpoint`) |

### Por ação

| Medida | Fórmula |
|---|---|
| LPA | lucro dos controladores dos últimos 12 meses ÷ total de ações da B3 |
| VPA | patrimônio dos controladores no fim dos 12 meses ÷ total de ações da B3 |

Prejuízo dá LPA negativo e patrimônio negativo, VPA negativo: o número sai, e
quem usa (o Graham) decide. Sem o número de ações: `no_shares`.

### Holding de participações

`holding: true` quando o cadastro põe a companhia no setor de administração de
participações (`SETOR_ATIV` começando por "Emp. Adm. Part.") **e** o resultado
de equivalência patrimonial dos 12 meses (`trailing`) é pelo menos **metade**
do lucro, em módulo. O setor sozinho não basta (WEG, CPFL, Neoenergia, BRF e
dezenas de grupos estão nele e consolidam as controladas: a margem deles vale);
a régua e a medida que a justifica estão em
[resultados.md](resultados.md#não-se-aplica-holding-banco-e-seguradora). Na
holding, as margens, a dívida líquida ÷ EBITDA e o CAGR da receita vêm com
`gap: "holding"` (o EBITDA dela leva a equivalência, que não é caixa nem
receita); o motivo de banco e seguradora (`financial_template`) tem
precedência. A classificação é da companhia: a série de cinco exercícios usa a
mesma régua.

### Recuperação judicial

`SIT_EMISSOR` do cadastro começando por "EM RECUPERAÇÃO JUDICIAL". O texto
inteiro da situação (falida, liquidação extrajudicial, em recuperação
extrajudicial, paralisada) também é exposto (`issuer_status`), para a tela e a
Análise mostrarem.

### FII

| Medida | Fórmula |
|---|---|
| Valor patrimonial por cota | `Patrimonio_Liquido ÷ Cotas_Emitidas` do informe mensal mais novo |
| P/VP | cotação do Prumo ÷ valor patrimonial por cota (a folha recalcula com o preço ao vivo) |
| Vacância física | média da `Percentual_Vacancia` dos **imóveis para renda acabados** do informe trimestral mais novo, **ponderada pela área** |

A vacância é a **física** (área vaga), a que a CVM publica imóvel a imóvel; a
financeira (receita perdida) não está nos dados abertos. Imóvel sem área ou com
vacância fora de 0 a 1 (erro de preenchimento: há valor negativo no arquivo)
fica fora da média. Fundo de papel ou de fundos não tem imóvel para renda
(`no_rental_property`); fundo fora do trimestral, `no_quarterly_report`.

## Motivos (`gap`)

| Código | Quer dizer |
|---|---|
| `financial_template` | banco ou seguradora: a conta não existe no plano |
| `unknown_template` | plano que o Prumo não reconhece |
| `not_reported` | a conta não veio (ou veio com outro conteúdo) |
| `no_filing` | sem balanço do exercício |
| `non_positive_base` | denominador zero ou negativo |
| `non_positive_endpoint` | CAGR com valor ≤ 0 numa ponta |
| `insufficient_history` | faltam exercícios na janela |
| `no_quote` | sem cotação (P/VP) |
| `no_rental_property` | FII sem imóvel para renda acabado |
| `no_quarterly_report` | FII fora do informe trimestral |
| `no_shares` | sem número de ações (B3) ou de cotas (informe), ou zero |
| `holding` | holding de participações: o lucro vem das participadas, a medida não se aplica |
| `implausible` | número informado fora do possível (rendimento mensal de FII negativo ou acima de 10%) |

E por que um ativo inteiro fica sem fundamentos (`reason`):

| Código | Quer dizer |
|---|---|
| `not_listed_on_b3` | EUA, cripto, Tesouro, renda fixa |
| `exchange_traded_product` | ETF ou BDR da B3 fora de Ações Nacionais e FIIs |
| `no_cvm_code` | a B3 não informa código CVM (ETF em Ações Nacionais) |
| `no_filings` | código CVM sem DFP nem ITR nos arquivos lidos |
| `fund_not_found` | FII fora do informe mensal |
| `pending` | a primeira busca ainda não terminou |

## Limitações

- **Holding** (Itaúsa, por exemplo) está no plano comercial, mas o lucro vem
  da equivalência patrimonial e não da receita: a margem líquida passaria de
  100%. Desde a migração 0031 a margem dela vem como `holding` ("não se
  aplica"); a régua está acima.
- **Ano fiscal fora do calendário** funciona (o exercício é o ano do fim do
  período), mas os 12 meses só são montados quando os períodos se encaixam.
- **Banco e seguradora:** só lucro, patrimônio, ROE, anos com lucro, CAGR do
  lucro e LPA/VPA. Índice de Basileia, inadimplência e índice combinado não
  estão nos arquivos abertos da CVM.
- **Ações em tesouraria** não são descontadas no LPA/VPA (ver acima).
- **Vacância** só física, e só do que o fundo declara.
- **Empresas dos EUA, BDR e ETF:** fora (não entregam DFP à CVM; o SEC EDGAR
  pede contato pessoal no User-Agent, ver `docs/saude.md`).
- **Dados atrasam:** a DFP sai até março do ano seguinte e o ITR até 45 dias
  depois do trimestre; o Prumo mostra a data do documento e a da conferência.

## Contrato para a Análise (estável)

Em Rust, para quem roda no servidor:

```rust
// backend/src/store/fundamentals.rs
impl Store {
    pub async fn fundamentals_for_user(&self, user: Uuid)
        -> Result<HashMap<String /* símbolo do ativo */, AssetFundamentals>, StoreError>;
}

// backend/src/domain/fundamentals.rs
pub enum AssetFundamentals {
    Company(Box<CompanyMetrics>),
    Fund(Box<FundMetrics>),
    Unsupported(Unsupported),
}
```

- A chave é `assets.symbol` (`WEGE3`, `KNRI11`); ativo sem símbolo (renda fixa
  por valor) não entra. ETF, BDR, cripto, EUA, Tesouro: `Unsupported(reason)`.
- `Company` e `Fund` vêm em `Box` (o `CompanyMetrics` é grande; sem a caixa, o
  `clippy::large_enum_variant` recusa o enum). `match` com `Company(m)` e `&**m`
  ou `m.as_ref()`.
- Toda medida é `Figure`: `figure.value() -> Option<Decimal>` e
  `figure.gap() -> Option<Gap>` (exatamente um dos dois). Razões como fração
  (`0.15` é 15%).
- `CompanyMetrics`: `cvm_code`, `name`, `template`, `holding`, `judicial_recovery:
  Option<bool>` (`None`: cadastro sem a companhia), `issuer_status`,
  `registration_status`, `years: Vec<FiscalYear>` (cinco, do mais velho ao mais
  novo; `figures: None` = ano faltando), `trailing: Option<PeriodFigures>` (os
  últimos 12 meses), `profitable_years` e `positive_operating_cash_years`
  (`YearCount { positive, known, window, gap }`), `revenue_cagr`,
  `earnings_cagr`, `latest_filing`, `shares: Option<Decimal>`,
  `earnings_per_share` (LPA), `book_value_per_share` (VPA), `synced_at`.
- `PeriodFigures`: `period_start`, `period_end`, `template`, `sources`,
  `revenue`, `net_income`, `equity`, `equity_controlling`, `gross_debt`,
  `cash`, `net_debt`, `ebit`, `depreciation_amortization`, `ebitda`,
  `operating_cash_flow`, `roe`, `net_margin`, `ebitda_margin`,
  `net_debt_to_ebitda`, `net_debt_to_equity`.
- `FundMetrics`: `cnpj`, `issuer`, `name`, `reference_date`, `net_assets`,
  `shares`, `nav_per_share` (o VPA da cota), `price`, `price_to_book`,
  `vacancy`, `vacancy_reference_date`, `synced_at`.
- Para o Graham (`analysis::PerShare { earnings, book_value }`):
  `earnings_per_share.value()` e `book_value_per_share.value()` da companhia.

Pela rede, para a tela: `GET /api/v1/fundamentals` (ver
[`api.md`](api.md#fundamentos)).
