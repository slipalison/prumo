# Tesouro Direto no Prumo

Título do Tesouro Direto cadastrado pelo nome ("Tesouro IPCA+ 2035"), com
quantidade fracionária, e cotado sozinho pelo **preço de resgate** do dia. A
renda fixa por valor (CDB, LCI) continua como era. Roteiro: `docs/roadmap.md`
§ 3.8; contrato: [`api.md`](api.md); como o aporte compra fração:
[`alocacao.md`](alocacao.md).

## A fonte

**Tesouro Transparente**, conjunto "Taxas dos Títulos Ofertados pelo Tesouro
Direto" (CKAN, dados abertos, licença ODbL, sem chave nem cadastro). Conferido
com `curl` em 25/11/2026:

| | |
|---|---|
| Conjunto | `https://www.tesourotransparente.gov.br/ckan/api/3/action/package_show?id=df56aa42-484a-4a59-8184-7676580c81e3` |
| Arquivo | `…/resource/796d2059-14e9-44e3-80c9-2d9e30b405c1/download/precotaxatesourodireto.csv` |
| Tamanho | 14,7 MB, ~178 mil linhas, de 2004 até a última divulgação |
| Formato | `;` como separador, vírgula decimal, datas `dd/mm/aaaa`, ASCII |
| Colunas | `Tipo Titulo;Data Vencimento;Data Base;Taxa Compra Manha;Taxa Venda Manha;PU Compra Manha;PU Venda Manha;PU Base Manha` |
| Última data-base vista | 19/11/2026 (arquivo modificado em 20/11/2026) |

O Prumo usa **`PU Venda Manha`**: o preço a que o Tesouro recompra o título da
pessoa — o resgate, que é o que o patrimônio vale. Também lê a **`Taxa Compra
Manha`**: dos títulos IPCA+ sai o juro real de referência (abaixo). `PU Compra
Manha` é o preço de quem compra (mais alto); `PU Base Manha` é o da data-base. A tabela traz só o
que o Tesouro oferece: hoje 58 títulos vigentes, de 8 tipos.

**O que ficou de fora.** O JSON do site `tesourodireto.com.br`
(`…/treasurybondsinfo.json`) responde `410 Gone` (medido em 25/11/2026), então
não é fonte. Nada aqui contorna bloqueio, Cloudflare ou CAPTCHA: o CSV responde
a um `GET` comum, com o mesmo cliente HTTP do resto do app (que se identifica
de forma genérica, sem dado da pessoa).

**Como o endereço é achado.** O CSV muda de endereço quando o Tesouro o
reenvia, então o Prumo pergunta ao CKAN (`package_show` do conjunto, pelo id,
que não muda) qual recurso é o CSV e usa **só o caminho** que ele devolve,
sempre no servidor configurado. Se o CKAN falhar ou não listar um CSV, usa o
endereço de 25/11/2026 (`KNOWN_CSV_PATH`) e registra um aviso no log.

**Atraso.** O Tesouro divulga com defasagem (em 25/11/2026 a última data-base
era 19/11). A tela mostra a data do preço ("preço de resgate de 19/11/2026"),
não a hora em que o Prumo baixou. É o preço da manhã daquele dia, não o do
momento do resgate.

## O que baixa, quando e quanto guarda

- **1× por dia.** Uma rodada confere a cada hora se a última gravação tem mais
  de 20 horas (`CHECK_EVERY`, `MAX_AGE` em `src/treasury.rs`); só então baixa. Um
  deploy ou reinício não baixa de novo se a tabela está em dia. Falhou: o preço
  de ontem continua valendo e a próxima conferência tenta de novo. As duas
  réplicas conferem de forma independente (no pior caso, cada uma baixa uma vez
  por dia; o gravador é idempotente).
- **Em fluxo.** O arquivo é lido por pedaços, linha a linha, sem guardar o CSV
  inteiro. De cada título que ainda não venceu fica só a linha de data-base mais
  recente com preço de resgate maior que zero (a ordem das linhas não importa;
  hoje o arquivo vem do mais novo para o mais velho, mas isso não é contrato).
- **Só o necessário no banco.** Uma linha de `quotes` por título vigente. Título
  que sai da tabela (venceu ou saiu de oferta) é apagado da `quotes`, **menos**
  se alguma carteira o guarda: esse fica com o último preço e a data dele, e a
  tela mostra a data. Uma tabela que vem sem nenhum título (formato mudou, página
  de erro) é erro e **não apaga nada**.
- **O histórico, para o Desempenho.** Cada rodada grava também, em
  `price_history`, o preço do dia de cada título que alguma carteira guarda. O
  passado (desde o primeiro lançamento do título no livro) sai do mesmo
  arquivo, lido em fluxo por `src/treasury/history.rs`, só quando falta dia de
  antes do que já está guardado. Detalhes em
  [`desempenho.md`](desempenho.md#fontes).
- As colunas são achadas pelo nome do cabeçalho: coluna nova ou fora de ordem
  não quebra a leitura; coluna que falta (`PU Venda Manha`) é erro. Tipo de
  título que o Prumo não conhece vira aviso no log, não erro.

## Juro real de referência

O alvo de rendimento do FII de tijolo é o juro real do Tesouro mais um prêmio
([`analise.md`](analise.md#fiis-vp-e-rendimento-pelo-tipo-do-fundo)). Ele sai do
mesmo arquivo, na mesma leitura em fluxo:

- De cada título vigente fica, junto do preço de resgate, a **taxa de compra**
  (`Taxa Compra Manha`, % ao ano) da linha mais nova — só quando o título está à
  venda no dia (`PU Compra Manha` maior que zero) e as duas colunas existem no
  cabeçalho; sem elas o preço continua sendo lido e só não há taxa.
- Entre os títulos **IPCA+** (`Tesouro IPCA+` e `Tesouro IPCA+ com Juros
  Semestrais`) da **data-base mais nova**, a rodada escolhe a **NTN-B** (com
  juros semestrais, o título que o mercado usa como "juro real de 10 anos") que
  vence **mais perto de 10 anos** depois da data-base; sem NTN-B, a NTN-B
  Principal mais perto de 10 anos; no empate, a mais longa
  (`domain::market_rates::pick_real_rate`). Em 26/11/2026, isso era a NTN-B
  15/05/2037, a 7,60%.
- Grava a taxa, o título e a data-base em **`treasury_real_rate`** (migração
  `0025`, uma linha só, compartilhada como `quotes`). Tabela sem nenhum IPCA+ com
  taxa não apaga a de ontem.
- A tela mostra "juro real 7,60% em 26/11/2026 (Tesouro IPCA+ com Juros
  Semestrais 2037)".

## Como o título entra no modelo

**Identidade.** Tipo + vencimento. A chave é o símbolo do ativo e da cotação:
`TD-<tipo>-<aaaammdd>`.

| Tipo no Tesouro | Código | Exemplo |
|---|---|---|
| Tesouro Selic | `SELIC` | `TD-SELIC-20290301` |
| Tesouro Prefixado | `PRE` | `TD-PRE-20270101` |
| Tesouro Prefixado com Juros Semestrais | `PREJS` | `TD-PREJS-20350101` |
| Tesouro IPCA+ | `IPCA` | `TD-IPCA-20350515` |
| Tesouro IPCA+ com Juros Semestrais | `IPCAJS` | `TD-IPCAJS-20320815` |
| Tesouro IGPM+ com Juros Semestrais | `IGPMJS` | `TD-IGPMJS-20310101` |
| Tesouro Renda+ Aposentadoria Extra | `RENDA` | `TD-RENDA-20491215` |
| Tesouro Educa+ | `EDUCA` | `TD-EDUCA-20371215` |

O nome que a pessoa lê é `Tesouro IPCA+ 2035`. Se dois títulos do mesmo tipo
vencem no mesmo ano, a data entra no nome (`Tesouro IPCA+ 2035 (venc.
15/05/2035)`). Código em `src/domain/treasury.rs`.

**Mercado.** `Market::Treasury` (`src/domain/asset_class.rs`): unidade
`UnitKind::Hundredth` (fração de centésimos de título), símbolo na fonte igual à
chave, fora do provedor Yahoo e do streamer.

**Marca no ativo (migração `0008_treasury.sql`).** O mercado de um ativo sai da
classe e de `listed_on_b3` (`AssetClass::market`). O título é Renda Fixa, com
símbolo e fora da B3: nenhuma das duas marcas o diz. Escolhi uma coluna nova,
`assets.treasury boolean`, em vez de duas alternativas:

- *Deduzir da forma do símbolo* (`TD-…`): esconde a regra, e um ticker digitado
  vira título sem querer. Com a coluna, Renda Fixa com símbolo e sem a marca
  continua sendo a de B3.
- *Trocar `listed_on_b3` por uma coluna de mercado* com várias opções: o desenho
  certo se vierem mais mercados, mas mexe em toda consulta, DTO e tela que já
  usam a marca, e as frentes em andamento mexem nelas.

A restrição `assets_treasury_is_fixed_income` impede os estados que não existem
(título fora de Renda Fixa, sem símbolo ou da B3). `AssetClass::market_of(listed,
treasury)` junta as duas marcas; `AssetClass::market(listed)` segue como era.

**Cotação.** O título entra em `quotes` como as outras cotações (moeda `BRL`,
nome do título em `name`, preço de resgate em `price`, data-base à meia-noite de
Brasília em `traded_at`). O cálculo de preço e valor da carteira não tem caso
especial: `price_source: "quote"`, `unit_price`, `current_value = quantidade ×
preço`. Quem mudou preço recebe o aviso no stream ao vivo como em qualquer
cotação.

**Fora do provedor.** `Market::from_provider()` é falso no Tesouro: o refresh
periódico e o manual, e a assinatura do streamer (`live/mod.rs`, `wanted`), o
pulam sem erro nem aviso no log.

## Busca e tela

`GET /api/v1/symbols` junta, na frente do resultado do provedor, os títulos
vigentes cujo nome casa com o que foi digitado (cada palavra aparece no nome,
em qualquer ordem, 3 caracteres no mínimo): `tesouro`, `ipca`, `ipca 2035`,
`selic`, `prefixado`, `renda+`, `educa+`. Até 10, com `suggested_class:
"fixed_income"` e `treasury: true`. Só aparecem os vigentes e ainda na tabela.

Na tela, **Novo ativo**: digitar "tesouro" (ou "ipca"…) na busca mostra os
títulos com o nome e "Tesouro Direto"; escolher um leva a classe (Renda Fixa) e
o modo junto. Em Renda Fixa há também um seletor **Por valor / Tesouro
Direto**: o segundo troca o campo por uma busca de título, que só cadastra com
um título escolhido da lista (a chave vai no cadastro; a pessoa vê o nome).
Quantidade fracionária, sem − e +, vírgula aceita; "0" vale (título ainda não
comprado, entra na divisão do aporte pela nota). Na carteira o título aparece
pelo nome, com "Tesouro Direto", quantidade em títulos e o preço; na folha de
edição, "preço de resgate de 19/11/2026". Aviso do plano, cartão do aporte e
histórico também falam o nome, nunca a chave.

## Ativo por valor que já existe: não converte

Quem já tem "Tesouro IPCA+ 2035" cadastrado por valor **não ganha conversão
automática**. O valor informado e a quantidade de títulos são grandezas
diferentes: converter exigiria adivinhar a quantidade (valor ÷ preço do dia, com
um valor que a pessoa atualizou à mão em outra data) e perder a diferença entre
o que ela anotou e o que o Tesouro paga. O servidor já recusa trocar o tipo de
cadastro no `PATCH` (o que vale para o ativo de B3 vale aqui). Para migrar:
cadastre o título com a quantidade que o app do Tesouro mostra e remova o antigo.
Nada quebra: os dois convivem na mesma classe, e o motor trata cada um pela sua
unidade (valor em centavos × título em centésimos).

## Limites conhecidos

- Preço da **manhã** (`PU Venda Manha`) de uma data-base com alguns dias de
  atraso; não é o valor líquido do resgate (imposto de renda, IOF, taxa de
  custódia da B3 ficam de fora, como no resto do Prumo).
- O Tesouro vende a partir de 0,01 título e R$ 30; o motor respeita os
  centésimos, mas **não** o valor mínimo de compra.
- O Tesouro Transparente pode mudar o arquivo; o cabeçalho é conferido e o erro
  aparece no log (`tabela do tesouro nao atualizada`), com o último preço
  mantido.

## Operação

- **Host novo de saída:** `www.tesourotransparente.gov.br` (HTTPS, porta 443),
  no `egress` do deploy.
- Sem variável de ambiente nova: segue `QUOTES_ENABLED` (`false` desliga a
  rodada do Tesouro junto com o provedor).
- Conferir na mão: `curl -sS
  "https://www.tesourotransparente.gov.br/ckan/api/3/action/package_show?id=df56aa42-484a-4a59-8184-7676580c81e3"`
  traz o endereço do CSV; as linhas mais novas estão no começo do arquivo.
- Logs: `tabela do tesouro atualizada` (com `bonds` e `pruned`),
  `tabela do tesouro nao atualizada` (erro), `ckan nao disse onde esta o csv do
  tesouro` (usou o endereço conhecido).

## Testes

- Leitura da tabela: unidade em `src/treasury.rs` (ordem das linhas, vencidos,
  preço zero, BOM/CRLF/colunas fora de ordem, linhas ilegíveis, arquivo sem
  preço, taxa de compra só com o título à venda, arquivo sem as colunas de
  compra), a escolha do juro real em `src/domain/market_rates.rs` (NTN-B mais
  perto de 10 anos, com proptest) e contra servidor de mentira em
  `tests/treasury_feed.rs`, com um recorte
  do CSV real (`tests/data/precotaxatesourodireto_amostra.csv`): descoberta pelo
  CKAN, endereço conhecido, falha de rede, página de erro e um arquivo grande
  lido em fluxo.
- Domínio: `src/domain/treasury.rs` (chave, nomes, busca, com proptest) e
  `src/domain/allocation.rs` (centésimos de título, com a propriedade do motor
  incluindo títulos).
- Postgres de verdade: `tests/treasury_api.rs` — a tabela vira cotação e juro
  real (a NTN-B 2032 do recorte, a 7,77%, que a Análise mostra), a busca
  sugere, o título é cadastrado com preço, o plano compra fração, o streamer e o
  provedor o ignoram, e o que sai da tabela só some se ninguém o guarda.
- Telas: `PortfolioScreen.test.tsx` e `ContributeScreen.test.tsx`.
