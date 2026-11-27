# Painel da Carteira

Três blocos na tela Carteira: a faixa de indicadores do mês, a composição e
"Hoje na carteira". Informa o que aconteceu; não recomenda nada.

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

## Hoje na carteira (`GET /portfolio/day-reference`)

O servidor devolve, por ativo cotado, o **fechamento anterior**: o preço na
última foto diária anterior ao dia da cotação (fim de semana: cotação de sexta
mede contra quinta). Sem referência: ativo por valor, preço manual, foto a mais
de 7 dias da cotação ou cotação com mais de 5 dias. A tela mede o preço ao vivo
contra isso: variação % por ativo, e por classe (ganho em R$ = quantidade atual
x diferença de preço, sobre a posição ao fechamento anterior). Mostra as 5
maiores altas e baixas; filtro por classe. Em ativo no exterior a variação
inclui o câmbio (valores em reais).
