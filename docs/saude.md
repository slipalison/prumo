# Saúde da carteira e setor

> A Saúde virou a **Análise da carteira** ([analise.md](analise.md)), com mais
> regras e dimensões. As sete regras deste documento continuam valendo, com os
> mesmos nomes, limites e fórmulas, dentro de Diversificação (as cinco primeiras)
> e Dividendos (as duas de renda); `GET /health` segue respondendo. A tela é a
> Análise; esta página continua sendo a referência das fontes de setor e dos
> detalhes dessas sete regras.

Um painel de **avisos** sobre o que a meta por classe não enxerga: concentração
dentro da classe, num setor, classe rasa ou vazia, e a renda (todo mês, e sem
depender de um pagador só). O código das regras é
`backend/src/domain/health.rs` (puro, sem banco nem rede); as rotas estão em
[api.md](api.md); a tela, em [design.md](design.md).

## O que não é

- **Não há nota de 0 a 100**, nem peso, nem "saúde 72". Cada regra é uma
  pergunta de sim ou não com um limite que a pessoa escolhe.
- **Não copia indicador de ninguém.** As regras e os limites de fábrica são do
  Prumo; os números são só um ponto de partida, e a pessoa liga, desliga e muda
  cada um.
- **Não manda vender.** O "o que fazer" de cada aviso é aportar fora do que
  passou do limite (o `needed_contribution`), que é o que o resto do Prumo já
  sabe fazer.

## As regras

"Ativo" é o que tem **valor** (`value > 0`): um ativo cadastrado e ainda não
comprado não diversifica nada. A carteira é o total desses ativos. Passou do
limite é **estritamente maior** (no limite exato está tudo bem), comparando os
percentuais já arredondados em duas casas, os mesmos que a tela mostra.

| Regra (`rule`) | Pergunta | Limite de fábrica |
|---|---|---|
| `max_asset` | Algum ativo passa de X% da carteira? | 15% |
| `top_five` | Os 5 maiores juntos passam de Y% da carteira? | 60% |
| `max_sector` | Algum setor passa de Z% da classe, em Ações Nacionais e em FIIs? | 40% |
| `min_assets` | Alguma classe com meta tem menos de N ativos? | 3 |
| `empty_class` | Alguma classe com meta não tem nenhum ativo? | sem limite |
| `income_every_month` | Em quantos dos últimos 12 meses fechados caiu provento? | 12 meses |
| `max_payer` | Algum pagador passa de W% da renda recebida em 12 meses? | 25% |

Cada uma liga e desliga sozinha e, quando tem limite, ele é editável
(`PUT /health/rules`). Os de fábrica são conservadores de propósito: melhor a
pessoa subir o limite do que achar que a regra "não pega nada".

Detalhes que mudam o resultado:

- **`top_five`** só vale com **mais de 5 ativos**: com 5 ou menos, os cinco
  maiores são a carteira toda (100%) e a regra não diria nada. O aviso lista os
  cinco, do maior para o menor.
- **`max_sector`** olha **dentro da classe**, não da carteira: "Financeiro é
  83% das suas Ações Nacionais". Só vale em `br_stocks` e `br_reits`, as únicas
  com fonte de setor, e só num grupo com **2 ativos ou mais** (um ativo é
  sempre 100% do próprio setor; dizer isso não ajuda). Ativo **sem setor entra
  no denominador** (conta como parte da classe, não de nenhum setor): a fatia
  de um setor nunca é inflada por quem a fonte não conhece.
- **`min_assets`** só olha a classe com meta que **já tem algum ativo**; a
  classe sem nenhum é assunto de `empty_class`. Evita dois avisos para o mesmo
  buraco.
- **`empty_class`** conta a classe com meta e **nada comprado** (nenhum ativo
  com valor), inclusive a que tem ativos cadastrados e ainda sem compra. O "o
  que fazer" é cadastrar (se falta) e usar o próximo aporte nela.
- Uma regra ligada que ainda não tem o que medir (carteira vazia, poucos
  ativos, nenhuma meta, nenhum setor conhecido) aparece como **"Sem o que
  medir"**, nunca como "tudo certo".

### Quanto aportar para voltar ao limite

Nas regras de percentual, o aviso traz `needed_contribution`: o dinheiro novo,
aplicado **fora** do que passou, que traz o percentual de volta ao limite.
Para uma parte `p` de um total `T` e um limite `L%`:

```
p / (T + x) = L / 100   =>   x = p × 100 / L − T
```

Em `max_sector`, `T` é o total da classe e `x` vai para outros setores dela.
Nenhuma venda é sugerida.

## Setor e segmento

Fontes públicas e gratuitas, guardadas numa tabela de referência
(`sector_reference`, compartilhada entre as carteiras como `quotes`) e
atualizadas **a cada 30 dias**. Depois de uma falha, o serviço espera 6 horas
antes de tentar de novo. Uma busca que volta vazia, ou com menos da metade das
linhas da última boa, é recusada: a referência anterior continua valendo.

**Fonte fora do ar não quebra a carteira**: o setor fica como estava (ou
vazio, se nunca veio) e o painel mostra "A última busca falhou".

A chave é o **código de quatro caracteres do emissor**: `WEGE3` e `WEGE4` →
`WEGE`; `KNRI11` → `KNRI`; `PETR4F` (fracionário) → `PETR`.

### Ações da B3: Classificação Setorial

Planilha pública que o botão de download da página "Classificação setorial" da
B3 entrega. A página é um aplicativo que chama

```
GET https://sistemaswebb3-listados.b3.com.br/listedCompaniesProxy/CompanyCall/GetDownloadIndustryClassification/<base64 de {"language":"pt-br"}>
```

e recebe um `.xlsx` (355 emissores em 25/11/2026). O endereço antigo
(`bvmf.bmfbovespa.com.br/InstDados/InformacoesEmpresas/ClassifSetorial.zip`)
ainda responde, mas com uma planilha de setembro de 2022; não é usado. A
planilha traz setor, subsetor e segmento (só na primeira linha de cada grupo) e
o código do emissor. O Prumo guarda os três.

### FIIs: informe mensal da CVM

`https://dados.cvm.gov.br/dados/FII/DOC/INF_MENSAL/DADOS/inf_mensal_fii_AAAA.zip`
(ano de agora; o anterior, se o novo ainda não saiu). Três CSV por ano; o Prumo
lê dois:

- `geral`: o **ISIN** da cota (`BRKNRICTF007` → `KNRI`, as quatro letras do
  ticker) e o `Segmento_Atuacao` que o administrador declara;
- `ativo_passivo`: o balanço, para dizer o **tipo**.

O ticker não vem no informe, só o ISIN, e é dele que sai a chave. Vale o
informe mais novo de cada fundo (`Data_Referencia`, depois `Versao`); dois
fundos com o mesmo emissor (mudança de CNPJ) ficam com o mais novo.

**Tipo (tijolo, papel, fundo de fundos, híbrido).** O campo `Mandato` da CVM
vem vazio e `Segmento_Atuacao` é, em cerca de três quartos dos fundos, `Multicategoria` ou
`Outros`. Por isso o Prumo olha o balanço do último informe e soma o que o
fundo tem em:

- **imóveis**: `Direitos_Bens_Imoveis` e ações/cotas de sociedades de atividades
  imobiliárias;
- **recebíveis**: CRI, LCI, LIG e letras hipotecárias;
- **cotas de FII**.

Dois terços ou mais dessa soma define o tipo (**Tijolo**, **Papel** ou **Fundo
de fundos**); sem nenhum com dois terços, **Híbrido**. Caixa e títulos públicos
ficam fora da conta; fundo sem nada nessas três contas (recém-criado) fica sem
setor. É o mesmo corte de dois terços que a B3 usa para holdings. **A regra é
do Prumo, não da CVM**, e é por ela que "Tijolo / Papel" aparece no lugar do
setor: em FIIs, `sector` é o tipo e `segment`, o que o administrador declara
(que pode destoar: o `Segmento_Atuacao` de alguns fundos de papel é
"Logística").

### Tesouro Direto e o resto: sem setor

O título do Tesouro Direto (`TD-…`, renda fixa) não tem setor B3 nem CVM: o
Prumo nem o procura e ele **não conta** como "sem setor" de ações ou FIIs
(`unclassified_count` olha só Ações Nacionais e FIIs). Nos avisos o título
aparece pelo nome, não pela chave. Cripto e renda fixa por valor idem.

### Ações dos EUA: sem setor

A fonte pública seria o código SIC do **SEC EDGAR**. A SEC exige que quem
consulta se identifique com um `User-Agent` com **nome e e-mail de contato**, e
bloqueia (`403`, "Request Rate Threshold Exceeded") o cliente genérico do
Prumo. O Prumo não envia nome, e-mail nem dado de ninguém a serviço externo e
**não contorna bloqueio**; então Ações Internacionais e REITs ficam **sem
setor**, e o painel diz isso. Para ligar, bastaria um contato institucional
configurado em variável de ambiente; não foi feito.

## Limitações conhecidas

- Cobertura: a planilha da B3 não lista todos os emissores (355 em 25/11/2026),
  e papel novo ou fundo sem informe fica "sem setor" até a fonte o conhecer. O
  painel diz quantos (`unclassified_count`).
- BDR, ETF e cripto não têm setor (a planilha classifica emissores da B3, não
  os ativos estrangeiros por trás de um BDR).
- O tipo do FII é uma fotografia do último informe (defasado de um a dois
  meses) e uma regra de corte: um fundo no limite entre tijolo e híbrido pode
  trocar de tipo de um mês para o outro.
- O limite único de `max_sector` vale para ações (setor da B3) e FIIs (tipo,
  só quatro valores). Para FIIs, 40% de "Tijolo" é quase sempre ultrapassado:
  quem quer a regra nos dois pode subir o limite, quem não quer, desligar.
- Os percentuais seguem o preço ao vivo: um aviso pode aparecer e sumir ao
  longo do dia perto do limite.

## Regras de renda

As duas últimas regras leem os **proventos recebidos** (`income_events`, os
mesmos da tela Renda: `pay_date` de hoje para trás, valor líquido, qualquer
origem). Não olham o que ainda vai cair nem a estimativa.

**Sem histórico, sem aviso.** O histórico é medido do mês do primeiro provento
recebido até o mês de hoje, inclusive. Com **menos de 3 meses**, as duas regras
ficam "Sem o que medir" (`not_applicable`), nunca "tudo certo" e nunca aviso:
quem acabou de cadastrar o primeiro provento não está com a renda falha.

### `income_every_month`

Conta, entre os **12 meses fechados** que antecedem o mês de hoje, em quantos
houve provento recebido (valor líquido maior que zero). Avisa quando a conta é
**menor** que o limite (padrão 12; a pessoa escolhe de 1 a 12).

- O mês de hoje fica de fora de propósito: ele ainda está pagando, e a regra
  avisaria todo dia 1º. A janela é por isso diferente da "renda de 12 meses"
  da tela Renda, que termina no mês de hoje.
- Mês anterior ao primeiro provento da pessoa conta como vazio: quem começou a
  receber há cinco meses tem, de fato, renda em cinco dos 12.
- **O que fazer.** O aviso lista os meses vazios e, **pelo histórico**, quais
  ativos da carteira já pagaram naquele mês do calendário em outro ano (ou no
  mês de hoje), com a classe e os meses. Só entra ativo que **ainda está na
  carteira com valor**; o Prumo não sugere ativo de fora. Se nenhum pagou, o
  aviso diz isso e leva à tela Renda. Não há aporte calculado: o histórico diz
  quem paga quando, não quanto aportar.

### `max_payer`

Soma o líquido recebido nos 12 meses que terminam no mês de hoje (a mesma
janela e o mesmo agrupamento por pagador da tela Renda) e avisa cada pagador
**estritamente acima** do limite (padrão 25%, sobre percentuais arredondados em
duas casas, como as outras regras).

- Com **um só pagador** a regra fica "Sem o que medir": ele seria sempre 100% e
  dizer isso não ajuda (mesmo critério do setor com um ativo só).
- O pagador é o ativo; se o ativo foi apagado da carteira, vale o símbolo do
  provento e o aviso vem sem `asset_id`.
- O aviso traz quanto o pagador pagou e o total (em reais; o olho fechado os
  mascara), mas não um aporte: espalhar a renda é aportar em outros ativos que
  paguem, e qual é escolha da pessoa.

### Quem já usava o painel

`health_rules` guarda só o que a pessoa mexeu. A migração 0016 apenas amplia o
que a tabela aceita; as linhas existentes ficam como estão. Quem nunca mexeu
nas duas regras novas passa a vê-las ligadas, com 12 meses e 25%.

## Operação

- Variável `SECTORS_ENABLED` (padrão `true`): `false` desliga as buscas (os
  testes e o desenvolvimento sem rede usam).
- Hosts externos: `sistemaswebb3-listados.b3.com.br` e `dados.cvm.gov.br`, só
  `GET` por HTTPS, a cada 30 dias, com o `User-Agent` genérico do app.
- Tabelas: `sector_reference`, `sector_sync` (estado de cada fonte) e
  `health_rules` (só o que a pessoa mexeu; o resto é o padrão do código).
- As amostras reais dos testes estão em `backend/tests/fixtures/` (ver o
  `README.md` de lá, com a data de gravação).
