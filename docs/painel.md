# Painel da Carteira

Quatro blocos na tela Carteira: a faixa de indicadores do mês, o cartão
"Resultado do dia" na faixa do patrimônio, a composição e "Hoje na carteira".
Informa o que aconteceu; não recomenda nada.

## Faixa de indicadores (`GET /portfolio/month`)

Tudo sai da série diária (`portfolio_snapshots`, mais a foto parcial de hoje).

- **Patrimônio no mês**: patrimônio ao vivo menos o patrimônio do último dia
  do mês anterior (e o percentual sobre ele). A conta final é feita na tela com
  o total ao vivo; o servidor entrega o patrimônio do fim do mês anterior.
  Inclui aportes e proventos.
- **Rentabilidade**: TWR pela cota, do fechamento do mês anterior até hoje; ao
  lado, a do mês anterior (do fechamento de dois meses atrás ao do mês passado).
  Série que começou no meio do mês mede da primeira cota (`full_window: false`).
- **Proventos**: soma do `income` (líquido) dos dias do mês; o anterior, inteiro.
- **Aporte**: soma do `net_flow` dos dias (aportes menos retiradas).

Mês em andamento contra mês fechado tende a mostrar menos; o ⓘ avisa. O sentido
vem escrito ("acima", "abaixo", "igual"), nunca só pela seta ou cor. Com os
valores ocultos, os reais viram máscara e a comparação deles some.
Conta: `backend/src/domain/panel.rs`.

## Composição (só tela, sobre `GET /portfolio`)

Árvore carteira → classe → ... com altura proporcional ao valor. Último nível
à escolha: **Setor** (em FIIs, o tipo do fundo), **Segmento** (pula o setor) ou
**Ativo** (carteira → classe → setor → ativo). Sem classificação da fonte,
"Sem setor"/"Sem segmento". No diagrama, fatias do último nível abaixo de 1%
da carteira, duas ou mais sob o mesmo pai, viram "Outros (n)"; rótulos que se
sobreporiam ficam de fora (a tabela traz tudo). SVG próprio, sem dependência.
No celular só há as barras empilhadas (uma por classe, legenda escrita). A
tabela "Ver como tabela" dá valor e fatia de cada item.

## Hoje na carteira e Resultado do dia (`GET /portfolio/day-reference`)

O servidor devolve, por ativo cotado, o **fechamento anterior**: o preço na
última foto diária anterior ao dia da cotação (fim de semana: cotação de sexta
mede contra quinta). Sem referência: ativo por valor, preço manual, foto a mais
de 7 dias da cotação ou cotação com mais de 5 dias.

Junto vai o que os lançamentos **depois** dessa foto mudaram no ativo:

- `previous_quantity`: as cotas que havia no fechamento — a quantidade de agora
  menos o que compras, vendas, resgates, desdobramentos, grupamentos e
  bonificações trouxeram ou levaram depois dele. Saldo inicial e ajuste não
  descontam: dizem a quantidade que a pessoa já tinha.
- `bought`: o que as compras custaram, com as taxas, em reais (câmbio do
  lançamento).
- `sold`: o líquido das vendas e resgates (preço × quantidade − taxas).

Amortização fica de fora, como provento. Ativo que não estava na foto (ou
estava sem preço) só entra se teve compra ou venda depois dela, com
`previous_close: null`.

**A conta, na tela, com o preço ao vivo.** Por ativo:

```
resultado = quantidade_agora × preço_agora
          − previous_quantity × previous_close
          − bought + sold
base      = previous_quantity × previous_close + bought
```

Sem `previous_close`, as cotas que já havia entram pelo preço de agora (não há
o que medir nelas). É a variação do preço nas cotas do fechamento mais o ganho
de cada negócio contra o próprio preço: a compra do dia não ganha o pulo desde
o fechamento de ontem, e a venda realiza até o preço de venda. Proventos não
entram.

- **Resultado do dia** (cartão na faixa do topo, ao lado do Patrimônio): a soma
  dos ativos medidos, em reais, e em % sobre a soma das bases (duas casas).
  Diz contra qual fechamento mediu (o mais recente). Com os valores ocultos, o
  real vira máscara e o percentual fica. Sem nenhum ativo medido, "—"; com a
  API fora, o cartão some.
- **Hoje na carteira** (painel ao lado da composição): o mesmo total na
  primeira linha, o dia por classe (resultado e % sobre a base da classe) e as
  5 maiores altas e baixas pela **variação do preço** (preço agora ÷
  fechamento − 1); filtro por classe. Ativo comprado pela primeira vez no dia
  e ativo vendido inteiro contam no total e na classe, mas não nas listas.

Em ativo no exterior a variação inclui o câmbio (valores em reais).
