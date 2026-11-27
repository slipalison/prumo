# Amostras gravadas das fontes de setor

Arquivos reais, baixados em **25/11/2026**, usados pelos testes dos clientes
(`src/sectors/` e `tests/sectors.rs`). Nenhum tem dado de carteira.

- `b3_classificacao_setorial.xlsx`: resposta inteira de
  `GET https://sistemaswebb3-listados.b3.com.br/listedCompaniesProxy/CompanyCall/GetDownloadIndustryClassification/eyJsYW5ndWFnZSI6InB0LWJyIn0=`
  (a planilha que o botão de download de "Classificação setorial" da B3 entrega;
  355 emissores).
- `cvm_inf_mensal_fii_2026.zip`: recorte de
  `https://dados.cvm.gov.br/dados/FII/DOC/INF_MENSAL/DADOS/inf_mensal_fii_2026.zip`.
  Ficaram só os dois CSV que o Prumo lê (`geral` e `ativo_passivo`), com o
  cabeçalho e a codificação ISO-8859-1 originais, onze fundos e os informes de
  setembro e outubro de 2026. As colunas de endereço, telefone, site e e-mail
  do administrador foram esvaziadas.

Para regravar, baixe de novo e ajuste os números esperados nos testes.

# Amostras da brapi (logos)

- `brapi_v2_quote_petr4.json`: resposta real de
  `GET https://brapi.dev/api/v2/stocks/quote?symbols=PETR4`, gravada em
  **26/11/2026**. PETR4 é um dos quatro tickers que a brapi atende sem token (o
  sandbox dela); com token, o formato é o mesmo. O campo que o Prumo lê é
  `results[].data.logourl`. Os testes trocam `https://icons.brapi.dev` pelo
  endereço do servidor falso.
- `logo_exemplo.svg`: logo **fictício** no formato dos que a brapi serve
  (`<svg width="56" height="56">` com um fundo e caminhos preenchidos). Os logos
  reais são marcas de terceiros e não entram no repositório; o saneador foi
  conferido à parte contra 24 logos reais (ações e FIIs), todos aceitos sem
  mudança.
- `logo_malicioso.svg`: SVG **fictício** com `onload`, `<script>` e
  `<foreignObject>` com `javascript:`. Nenhuma parte dele pode chegar à resposta.
- Sem token a brapi responde 401 com
  `{"error":true,"message":"Token de autenticação não fornecido","code":"MISSING_TOKEN"}`
  (medido em 26/11/2026).

# Amostras do IFIX (estatísticas históricas da B3)

Respostas reais de
`GET https://sistemaswebb3-listados.b3.com.br/indexStatisticsProxy/IndexCall/GetPortfolioDay/<base64>`,
com `{"index":"IFIX","language":"pt-br","year":"<ano>"}` em base64, gravadas
em **27/11/2026** às 14h de Brasília (sem login nem chave; é a tabela da página
"Estatísticas históricas" do IFIX). Dado público de índice, nenhum de carteira.

- `b3_ifix_2025.json`: o ano inteiro, 250 pregões.
- `b3_ifix_2026.json`: até 26/11/2026; o pregão de 27/11, ainda aberto, não
  aparece.
- `b3_ifix_2010.json`: o primeiro ano, só com a base de 1.000 pontos em
  30/12/2010.
- `b3_ifix_empty.json`: um ano antes do índice existir (1999).
