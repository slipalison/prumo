# Como o Prumo decide o aporte

O código é `backend/src/domain/allocation.rs`; este documento explica o porquê
de cada passo. A rota é `POST /api/v1/plans` ([contrato](api.md)).

## A pergunta

> Tenho `A` reais para investir hoje. O que compro para a carteira ficar o mais
> perto possível das minhas metas?

As **metas** são percentuais por classe (somam 100). A **nota** de cada ativo
(0 a 10, do checklist ou manual) diz quanto da classe aquele ativo deveria
ocupar em relação aos outros da mesma classe.

## Os seis passos

### 1. O tamanho da carteira depois do aporte

`N = patrimônio atual + A`. A meta de cada classe em reais é `meta% × N`, e o
**que falta** nela (`gap`) é `max(0, meta − atual)`.

Usar `N` (e não o patrimônio de hoje) é o que faz o aporte de hoje já contar
para a meta de amanhã.

### 2. Quem pode receber

Uma classe é **elegível** quando tem pelo menos um ativo com nota > 0 e preço
conhecido. Classe abaixo da meta e sem ativo elegível fica **bloqueada**, e o
aviso diz o motivo (`reason`): nenhum ativo (`no_assets`), nenhum com nota
(`unrated`) ou os com nota sem preço (`unpriced`).

O ativo não precisa existir na carteira para ser elegível: cadastrado com
quantidade 0, ele vale 0 hoje, tem nota e preço, e recebe a fatia dele. É assim
que a pessoa monta a lista do que quer ter numa classe ainda vazia — sem isso,
uma carteira concentrada numa classe acima da meta não teria para onde mandar
o aporte, e ele inteiro voltaria como `unallocated`. O motor nunca manda o
dinheiro para a classe que já passou da meta para "não sobrar": isso desfaria
a diversificação que ele existe para manter.

### 3. Entre classes: proporcional ao que falta, com teto no que falta

O aporte é dividido entre as classes elegíveis na proporção do `gap` de cada
uma. Nenhuma classe recebe mais que o próprio `gap`.

Como `Σ metas = N` e `Σ atual = N − A`, vale sempre:

```
Σ gap positivo = A + Σ excesso das classes acima da meta  ≥  A
```

Então o aporte **sempre cabe** no que falta — exceto quando parte do que falta
está numa classe bloqueada. Só nesse caso sobra dinheiro depois de encher as
elegíveis, e essa sobra volta como `unallocated`, com um aviso
`class_blocked` dizendo qual classe precisa de um ativo.

**Por que não espalhar a sobra entre as outras classes?** Porque isso
empurraria classes elegíveis para cima da meta — exatamente o que a carteira
não quer. Dinheiro sem destino é informação ("cadastre um ativo internacional"),
não algo a esconder.

### 4. Dentro da classe: a nota vira fatia

A fatia ideal de cada ativo é `nota / Σ notas da classe`. Com quatro ativos de
nota 6 e um de nota 2, os de nota 6 deveriam ter 6/26 da classe cada e o de
nota 2, 2/26.

Ativos sem nota ou sem preço continuam ocupando espaço: o valor deles é
descontado antes de dividir a classe entre os elegíveis. O dinheiro vai para
quem está abaixo da própria fatia, na proporção do que falta a cada um.

**O % ideal que a Carteira mostra** é esta mesma fatia, `nota / Σ notas`, em
percentual: `ideal_class_percents` em `allocation.rs`, e o motor e a tela
passam pela mesma função de fatia. Ativo sem nota ou sem preço tem ideal 0, e
numa classe com algum elegível os ideais somam 100 (conferido por teste de
propriedade). Com `KNRI11` e `HGLG11` de nota 6, `BTLG11` e `TRXF11` de nota 4 e
`MXRF11` de nota 0: 30%, 30%, 20%, 20% e 0%.

### 5. Unidades compráveis

A unidade vem do **mercado** do ativo, não da classe:

| Mercado | Unidade |
|---|---|
| B3 (ações, FIIs, ETFs, BDRs) e bolsa americana | inteira, arredondada para baixo |
| Cripto | até 8 casas |
| Título do Tesouro Direto | centésimos de título (0,01) |
| Renda fixa por valor | centavos |

O **título do Tesouro Direto** é Renda Fixa na carteira (a meta e a fatia são
as da classe), mas tem mercado próprio: a quantidade é fracionária — o Tesouro
vende a partir de 0,01 título — e o preço é o de resgate da tabela diária
(`docs/tesouro.md`). O motor compra o que cabe em centésimos de título, para
baixo, e o que sobra do arredondamento (no máximo um centésimo de título,
algo como R$ 25 num título de R$ 2.500) entra no troco ou vai para o ativo
fracionável mais atrasado. Não modela o valor mínimo de compra do Tesouro
(R$ 30): quem monta o aporte confere na hora de comprar.

Quase sempre o mercado é o natural da classe. A exceção é o ativo marcado como
negociado na B3 (`listed_on_b3`) fora de Ações Nacionais e FIIs: um ETF de
bitcoin da B3 conta em Criptomoedas — a meta e a fatia são de cripto —, mas sai
em cota inteira e cotado em reais.

### 6. O troco

Arredondar para baixo deixa troco. Em vez de devolvê-lo, uma rodada gulosa:

1. Entre os ativos de unidade inteira com preço ≤ troco, compra **mais uma
   unidade** do que estiver mais atrás da meta — **desde que** isso o aproxime
   da meta (o que falta a ele é maior que meio preço). Repete.
2. Quando nenhuma unidade inteira couber, o resto vai para o ativo
   **fracionável** (cripto, título do Tesouro, renda fixa por valor) mais
   atrasado, até o que falta a ele.
3. O que sobrar depois disso é `change`.

## O que o resultado garante

Conferido por teste de propriedade (300 carteiras aleatórias a cada execução):

- `amount = invested + change + unallocated`, ao centavo.
- Ativo com nota 0 ou sem preço nunca recebe.
- Nenhum ativo passa da própria meta por mais de meia unidade.

E por dois cenários fixos (carteira fictícia de R$ 120 mil):

- **R$ 6.000**: cripto (muito abaixo da meta) leva a maior parte, os FIIs mais
  atrasados recebem cotas inteiras, e o troco fica em centavos.
- **R$ 200.000**: as classes elegíveis param na meta, e o que caberia na classe
  sem ativo aparece como `unallocated` — R$ 95.994,15, que é o buraco dela
  (R$ 96.000) menos o excesso de outra classe (R$ 5,85).

## Problemas que este desenho evita

Vistos em calculadoras de rebalanceamento por aí:

| Problema | Aqui |
|---|---|
| Limitar cada classe a `meta × N` em vez de `meta × N − atual`: com aporte grande, a classe passa da meta | teto é o que falta |
| Dinheiro sem destino que some sem aviso | `unallocated` + `class_blocked` |
| Troco do arredondamento perdido | rodada gulosa + fracionável |
| Nota tratada só como ordem de prioridade | nota é peso da fatia |
