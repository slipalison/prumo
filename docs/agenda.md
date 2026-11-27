# Agenda

A tela **Agenda** (`#/agenda`) junta, numa linha do tempo e num calendário
mensal, o que acontece com os ativos que a pessoa **tem na carteira**: data com
e pagamento dos proventos, o provento anunciado fora do padrão, o resultado
entregue à CVM, fato relevante, comunicado ao mercado, aviso aos acionistas e
cotistas, assembleia e os dias sem pregão da B3. Filtra por tipo de evento,
classe e ativo. **Informa, não recomenda**: nenhum evento diz o que fazer, e o
"fora do padrão" é uma conta contra o próprio histórico do ativo.

Nada sai por Telegram: a pessoa disse que não usa. Tudo está na tela.

Código: regra pura em `backend/src/domain/agenda.rs` e
`backend/src/domain/agenda/unusual.rs`; leitura dos arquivos e a tarefa em
`backend/src/agenda/` (`cvm.rs`, `fundos_net.rs`, `mod.rs`); banco em
`backend/src/store/agenda.rs` (migração `0028_agenda.sql`); rota em
`backend/src/http/agenda.rs`; tela em `frontend/src/features/agenda/`. Contrato
em [api.md](api.md#agenda).

## 1. Fontes

Conferidas em 27/11/2026, todas públicas, sem login, chave nem cadastro, pelo
`http_client()` do app (identificação genérica, nenhum dado da pessoa).

| O quê | Endereço | Tamanho | Atualização |
|---|---|---|---|
| Documentos eventuais das companhias (IPE) | `dados.cvm.gov.br/dados/CIA_ABERTA/DOC/IPE/DADOS/ipe_cia_aberta_<ANO>.zip` | ~2 MB compactado, ~17 MB aberto, ~40 mil linhas por ano | **semanal** (a CVM regera o ano corrente e o anterior aos domingos) |
| Entrega do ITR | `dados.cvm.gov.br/dados/CIA_ABERTA/DOC/ITR/DADOS/itr_cia_aberta_<ANO>.zip`, só a entrada `itr_cia_aberta_<ANO>.csv` | o arquivo tem ~30 MB; o índice é a **primeira** entrada do zip (~45 KB compactado), e o download para nela | semanal |
| Entrega da DFP | `.../DFP/DADOS/dfp_cia_aberta_<ANO>.zip`, só `dfp_cia_aberta_<ANO>.csv` | idem (~23 KB lidos de ~13 MB) | semanal |
| Documentos dos FIIs | `fnet.bmfbovespa.com.br/fnet/publico/pesquisarGerenciadorDocumentosDados?d=0&s=0&l=100&o[0][dataEntrega]=desc&cnpjFundo=<CNPJ>` (Fundos.NET, B3) | ~100 KB por fundo (os 100 documentos mais novos) | na hora em que o fundo entrega |
| Proventos | `b3_cash_announcements` (proventos automáticos) e `income_events` (Renda) | já no banco | de hora em hora |
| Dias sem pregão | `domain/tax/calendar.rs` | no código | anos cadastrados (hoje até 2027) |

**FII não tem IPE.** A CVM publica nos dados abertos de FII só os informes
mensal, trimestral e anual e as demonstrações (`dados.cvm.gov.br/dados/FII/DOC/`:
`DFIN`, `INF_ANUAL`, `INF_MENSAL`, `INF_TRIMESTRAL`). Fato relevante,
comunicado, aviso aos cotistas e assembleia de FII são entregues no
**Fundos.NET**, o sistema da B3 que a própria CVM indica para fundos; a página
de consulta é pública e responde em JSON. A Agenda consulta um fundo por vez,
pelo CNPJ que o informe mensal da CVM já dá (`cvm_funds`). Nada contorna
bloqueio: se o Fundos.NET recusar, a falha fica registrada e o gravado continua.

### O IPE

CSV com `;`, ISO-8859-1, sem aspas como delimitador (o mesmo leitor dos
fundamentos). Colunas:
`CNPJ_Companhia;Nome_Companhia;Codigo_CVM;Data_Referencia;Categoria;Tipo;Especie;Assunto;Data_Entrega;Tipo_Apresentacao;Protocolo_Entrega;Versao;Link_Download`.

| `Categoria` | Na Agenda |
|---|---|
| Fato Relevante | `material_fact` |
| Comunicado ao Mercado (todos os tipos: outros comunicados, apresentações a analistas, aquisição de participação relevante, esclarecimentos à CVM/B3...) | `market_notice` |
| Aviso aos Acionistas | `shareholder_notice` |
| Assembleia (edital, ata, proposta) | `meeting` |
| o resto (ata de reunião da administração, valores mobiliários negociados, políticas, estatuto, relatório de proventos...) | fora |

- O documento é identificado pelo `numSequencia` do `Link_Download` (o
  `Protocolo_Entrega` vem vazio em parte das linhas, e o arquivo repete
  algumas linhas idênticas: contam uma vez).
- O link do original é o `Link_Download` (RAD da CVM). Só um link
  `https://www.rad.cvm.gov.br/…` com caracteres de URL é aceito; o `http://`
  do índice do ITR/DFP vira `https://`. Outro endereço não é guardado.
- Reapresentação é outro documento, com outro número: aparece como outra linha.

### A entrega do ITR e da DFP

O índice de cada `.zip` de ITR e DFP tem uma linha por documento entregue:
`CNPJ_CIA;DT_REFER;VERSAO;DENOM_CIA;CD_CVM;CATEG_DOC;ID_DOC;DT_RECEB;LINK_DOC`.
O evento "Resultado" é o `DT_RECEB` (o dia em que o ITR ou a DFP chegou à CVM),
na versão mais alta, com o `LINK_DOC`. É a fonte certa: o IPE também tem
"Demonstrações Financeiras Intermediárias", mas só de um terço das companhias.
Ler só o índice corta o download: o leitor em fluxo para na primeira entrada e
a conexão fecha (`fundamentals::download`).

### O Fundos.NET

| `categoriaDocumento` (e `tipoDocumento`) | Na Agenda |
|---|---|
| Fato Relevante | `material_fact` |
| Comunicado ao Mercado | `market_notice` |
| Aviso aos Cotistas | `shareholder_notice` |
| Assembleia | `meeting` |
| Informes Periódicos › Demonstrações Financeiras ou Informe Trimestral Estruturado | `results` |
| Relatórios › Relatório Gerencial | `management_report` |
| Aviso aos Cotistas - Estruturado (rendimentos e amortizações), informe mensal e anual estruturado, regulamento... | fora (o provento vem da B3) |

Só o documento ativo (`situacaoDocumento = "A"`) entra; o substituído (`I`)
sai. O link é sempre `https://fnet.bmfbovespa.com.br/fnet/publico/exibirDocumento?id=<id>`,
montado do número, nunca copiado da resposta. Os 100 mais novos cobrem mais de
um ano de um FII ativo; o que o Fundos.NET deixou de listar dentro do período
que a resposta cobre (substituído ou retirado) sai do banco, e o mais velho
fica.

## 2. Quando busca

Uma tarefa em segundo plano confere **de hora em hora** o que venceu, na mesma
chave dos fundamentos (`FUNDAMENTALS_ENABLED`):

- **Arquivos da CVM** (IPE, índice do ITR e da DFP do ano corrente e do
  anterior: seis arquivos). Cada um é conferido **uma vez por dia** com GET
  condicional (`ETag`/`Last-Modified`; a CVM responde 304 quando nada mudou),
  com o estado em `cvm_sources` (chaves `documents:<ano>`,
  `receipts_quarterly:<ano>`, `receipts_annual:<ano>`), a reserva por réplica
  (`claimed_until`) e a regra de quando ler (`fundamentals::due_with`) dos
  fundamentos. Companhia nova na carteira, numa versão já lida, faz ler o
  arquivo de novo **só para ela**. Arquivo do ano que ainda não existe
  (janeiro) responde 404 e é conferido no dia seguinte.
- **Fundos.NET**: cada FII das carteiras uma vez por dia, reservado em
  `fund_document_fetches` (quem marca primeiro busca; a outra réplica pula),
  com 0,8 s entre fundos. Falha espera 6 h.
- **Só o que é de alguém.** As companhias são as das carteiras (pelo código
  CVM que a tarefa dos fundamentos acha na B3); os fundos, os FIIs das
  carteiras. Das ~40 mil linhas do IPE, ficam as das companhias da carteira.
- **Fonte fora do ar não quebra nada**: a falha fica registrada, o gravado
  continua, a próxima tentativa vem em 6 h. Arquivo sem as colunas esperadas é
  falha, não "nenhum documento".
- **Nenhuma conexão presa durante a rede**: toda gravação é uma transação curta
  com `pg_advisory_xact_lock`.
- Documento entregue há mais de 800 dias é apagado (a tela mostra até 400 dias
  de uma vez).

A leitura (`GET /agenda`) só lê o banco: nenhuma requisição espera fonte
externa.

## 3. Os eventos

| Tipo (`type`) | Dia | De onde |
|---|---|---|
| `income_record` (Data com) | a data com | provento da Renda; anúncio da B3 "a definir" |
| `income_payment` (Pagamento) | a data de pagamento | provento da Renda |
| `unusual_income` (Fora do padrão) | o dia do anúncio | anúncio da B3 (seção 4) |
| `results` (Resultado) | a entrega | ITR/DFP da companhia; demonstrações e informe trimestral do FII |
| `material_fact` (Fato relevante) | a entrega | IPE; Fundos.NET |
| `notice` (Comunicados e avisos) | a entrega | comunicado, aviso, assembleia, relatório gerencial |
| `market_closed` (Sem pregão) | o dia | calendário de pregões |

### Proventos

- **O que está na Renda.** Cada provento de `income_events` (lançado à mão,
  importado do extrato ou automático da B3) de um ativo que a pessoa tem
  cadastrado dá dois eventos: a data com e o pagamento, com o **líquido** que a
  Renda mostra (o JCP sem o IR retido, os 30% dos EUA...). Recebido ou a
  receber pela data de pagamento, como na Renda.
- **A data com** do automático é a do anúncio (está no `external_ref`); no
  lançado à mão, o pregão anterior à data ex (`previous_trading_day`, pelo
  calendário da B3). Sem data ex, só o pagamento aparece.
- **"A definir".** O anúncio da B3 ainda sem data de pagamento não vira
  provento automático (inventar a data poria o dinheiro no mês errado). Na
  Agenda ele aparece **só na data com**, com o líquido **estimado**: o valor por
  cota vezes as cotas do livro no fim da data com (`entitlement::quantity_on`),
  o JCP pela alíquota de hoje. Sem cota na data com, não aparece. Se a pessoa
  já lançou à mão o mesmo valor por cota (a 1%) com a mesma data com, não
  repete.
- Com os proventos automáticos **desligados**, a Agenda mostra o que está na
  Renda (o que a pessoa lançou ou importou) e os "a definir".

### Documentos

O documento aparece em cada ativo da carteira da mesma companhia (ITSA3 e
ITSA4 recebem o mesmo fato relevante numa linha só) e leva ao original
("Abrir", em nova aba). O resultado diz qual: "ITR do trimestre até
30/09/2026", "DFP de 2025".

### Dias sem pregão

Os dias **de semana** sem pregão da B3 (`trading_closures` em
`domain/tax/calendar.rs`): feriados nacionais, Carnaval, Corpus Christi, 24/12 e
31/12, com o motivo. Fim de semana não entra (o calendário já mostra). O dia sem
pregão não é de nenhum ativo: só o filtro de tipo o tira. Depois do último ano
cadastrado (hoje 2027) a tela avisa que o mês ainda não tem os dias sem pregão.

## 4. Provento fora do padrão

O anúncio da B3 cujo valor por cota se afasta do que o ativo costuma pagar —
**aumento ou corte** — vira um evento na Agenda (no dia do anúncio, o
`approvedOn` da B3) e um selo na data com, no pagamento e em **Próximos
pagamentos** da Renda ("Corte de 25%", "Aumento de 519% · 3 parcelas"). Uma
data com é **um** item, por mais parcelas que pague.

### A conta

1. **O pagamento**: o que a empresa declarou numa data com, **por cota de
   hoje** (cada bonificação, desdobramento e grupamento posterior divide o
   valor, a mesma base da estimativa da Renda, `shares_after`), **bruto**, com
   as **parcelas somadas** (o dividendo pago em 2026, 2027 e 2028 é um
   pagamento de 3 × 0,4128). Separado **por tipo** — dividendo, JCP,
   rendimento de FII — e no total. Amortização (devolve capital) e o "outro"
   da B3 numa ação (a correção do valor entre a data com e o pagamento) não
   formam padrão: entram só no total.
2. **O histórico**: por tipo, os anúncios da B3 (só ela diz o tipo; cobre de 12
   a 18 meses e cresce com o tempo); no total, a B3 e, antes dela, o Yahoo (10
   anos em cache, já na base de hoje, sem o tipo).
3. **Três maneiras de estar fora do padrão**, nesta ordem:

| Base (`basis`) | Quando | Referência |
|---|---|---|
| `similar` — corte ou aumento de um pagamento regular | o tipo tem pelo menos **2 pagamentos parecidos** (de metade ao dobro do valor) nos 12 meses antes da data ex | a média deles |
| `regular` — corte do pagamento regular | sem parecido, o valor fica **abaixo de todos** do tipo e **ocupa o lugar do pagamento regular**: o tipo tem pelo menos 2 pagamentos nos 12 meses, intervalo típico (mediana) de até 200 dias, e o pagamento anterior ficou a pelo menos 60% desse intervalo | a mediana do tipo nos 12 meses |
| `largest` — aumento extraordinário | o **total** da data com passa de `limiar ×` o **maior** pagamento dos 12 meses (com pelo menos 2 pagamentos neles) | o maior |

4. **Os 5 anos confirmam**: com 2 anos ou mais de histórico, o total da data
   com tem de dar o mesmo veredito contra os pagamentos de até 5 anos (os
   parecidos, com pelo menos **3**; o lugar do regular; ou o maior, com pelo
   menos 3 pagamentos). Com menos de 2 anos, só os 12 meses valem, como na
   média do habitual da Renda.
5. **O limiar**:

| Classe | Limiar | Aumento a partir de | Corte a partir de |
|---|---|---|---|
| FII | 1,25 | +25% | −20% |
| Ação (e o resto) | 1,5 (o mesmo do "acima do habitual") | +50% | −33% |

Pagamento que não cai em nenhuma das três — sem parecido e fora do lugar do
regular, ou entre dois padrões — é **avulso** e não vira nada: nem corte, nem
aumento. A tela diz o valor, as parcelas e a referência pela base ("R$ 0,60 por
cota; habitual de R$ 0,80 nos 12 meses e R$ 0,79 em 5 anos"; "R$ 1,2384 por
cota em 3 parcelas; o maior pagamento dos 12 meses foi de R$ 0,20").

### Por que assim

- **Parecidos, não todos.** Muita empresa paga em duas cadências: um banco com
  JCP mensal de R$ 0,018 e complementos de R$ 0,35 duas vezes por ano. A média
  de todos os pagamentos de um ano (R$ 0,065) faria todo JCP mensal parecer um
  corte de 72%. Com "de metade ao dobro", o JCP mensal é comparado aos mensais
  e o complemento aos complementos.
- **Nunca contra um pagamento só.** A primeira versão (27/11/2026) comparava o
  valor sem parecido com o **pagamento mais próximo**, e o `payments: 1` da
  resposta era isso — não um histórico curto: o pagador anual tinha os 5 anos
  no histórico, mas nenhum pagamento entre metade e o dobro de um resíduo de
  0,0046. Na carteira real isso deu dois "cortes" falsos (0,0046 contra 1,1166
  e 0,0449 contra 0,7302): um resíduo pequeno contra um único pagamento grande
  não é corte. Agora a referência tem pelo menos 2 pagamentos nos 12 meses e 3
  em 5 anos, a comparação é dentro do mesmo tipo (o JCP residual não é medido
  contra o dividendo), e o corte sem parecido só vale no **lugar** do pagamento
  regular: o FII que paga 0,30 no lugar dos 0,80 do mês é corte; o resíduo que
  cai três meses depois do dividendo anual, ou vinte dias depois do semestral,
  é avulso.
- **O aumento extraordinário não precisa de parecido.** Uma antecipação é, por
  definição, sem parecido: ela é marcada quando passa do maior pagamento dos 12
  meses (e dos 5 anos) pelo limiar. O corte, ao contrário, só existe contra um
  padrão.
- **Pagador anual**: um pagamento por ano não forma padrão em 12 meses, e ele
  não é avaliado.
- **Os dois períodos.** Pedir os 12 meses **e** os 5 anos no mesmo sentido
  corta o falso alarme de quem volta ao normal: o FII que pagou R$ 1,00 durante
  um ano extraordinário e volta aos R$ 0,80 de sempre não é "corte".
- **Limiar por classe.** O FII distribui quase todo o resultado do semestre e
  o rendimento mês a mês varia pouco: nas páginas públicas da B3 e no Yahoo
  (27/11/2026), FIIs mensais variaram 5–6% de um mês ao outro, e um corte de
  20% já é notícia para quem vive da renda. Em ação, o provento por data com
  segue o lucro e o calendário da empresa: com limiar de 1,25, elétrica,
  petroleira e banco seriam marcados a cada trimestre; com 1,5, sobram os de
  fato fora da curva.
- **Calibragem** (páginas públicas da B3 de 7 emissores — bancos com JCP mensal,
  FIIs mensais, elétrica, petroleira, holding —, 27/11/2026): nenhum FII mensal
  sem mudança é marcado; o JCP mensal e os complementos do banco ficam quietos;
  os testes com números fictícios no formato da carteira real
  (`domain/agenda/unusual.rs` e `tests/agenda.rs`) mostram o corte de 25% e o
  de 62% do FII mensal, o aumento extraordinário em três parcelas, e o resíduo
  avulso sem marca.
- Ativo sem anúncio da B3 (ETF, BDR, EUA) não é avaliado: não há "anunciado"
  antes do pagamento. Anúncio sem nada antes dele no histórico também não.

## 5. A tela

- **Mês**: a consulta é do primeiro ao último dia do mês escolhido (‹ ›,
  "Voltar para hoje").
- **Linha do tempo**: os dias com evento, em ordem; hoje sempre aparece
  (mesmo vazio); os dias que passaram ficam mais apagados.
- **Calendário**: o mês de domingo a sábado; cada dia é um botão com os tipos
  em pontos coloridos (o nome dos tipos está no rótulo do botão) e a lista do
  dia ao lado. Dia sem pregão é listrado.
- **Filtros**: os tipos (com quantos há no mês), a classe e o ativo (o ativo
  só oferece os da classe escolhida). "Limpar filtros" volta tudo.
- **Ocultar valores**: todo R$ (líquido do provento, valor por cota e habitual)
  passa por `useMoney()`, inclusive o texto só de leitor de tela.
- O ⓘ diz quando cada fonte foi conferida.

## 6. Limites

- **Fato relevante de companhia chega com até uma semana de atraso**: é o que
  os dados abertos da CVM dão (atualização semanal). O RAD da CVM tem consulta
  em tempo real, mas não como dado aberto; a Agenda usa só o que a CVM publica
  como aberto. FII é em tempo real (Fundos.NET).
- **A data de divulgação futura não existe como dado aberto.** As companhias
  entregam o "Calendário de Eventos Corporativos" no IPE, mas em PDF livre: a
  Agenda mostra o resultado quando ele é entregue, não a data prevista.
- **Proventos futuros** só aparecem quando a empresa anuncia (a B3 publica no
  anúncio). Não há projeção de provento na Agenda.
- **Fundos.NET** é uma página pública da B3, sem documentação formal: mudança
  de formato vira falha registrada (e o gravado fica), não dado errado.
- O "fora do padrão" com menos de 2 anos de histórico usa só os 12 meses, e o
  pagador anual não é avaliado (um pagamento por ano não forma padrão).

## 7. Banco (migração 0028)

| Tabela | O quê |
|---|---|
| `corporate_documents` | documento entregue (`cvm_ipe` por código CVM, `fundos_net` por CNPJ): categoria, título, assunto, data de referência, entrega, versão, link |
| `cvm_filing_receipts` | entrega de cada ITR e DFP das companhias da carteira (versão mais alta) |
| `fund_document_fetches` | a vez de cada réplica buscar um fundo e a última falha |
| `cvm_sources` (já existia) | o estado de cada arquivo da CVM, com as chaves da Agenda |

Tudo é referência compartilhada entre as carteiras (como os fundamentos): o
que filtra pela pessoa é a leitura, pelos ativos dela.
