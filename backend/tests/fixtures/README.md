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
