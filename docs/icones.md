# Logos dos ativos

Cada ação e cada FII da B3 mostra o logo da empresa ou do fundo, como nos apps
de carteira. O que não tem logo (Tesouro, cripto, renda fixa por valor, ativos
dos EUA) usa um ícone próprio ou as iniciais do ticker na cor da classe
([design.md](design.md#ícone-do-ativo)). Contrato das rotas em
[api.md](api.md#logos-dos-ativos).

## Como ligar

1. Crie uma conta gratuita em <https://brapi.dev> e copie a chave no painel
   (`/dashboard`).
2. Guarde a chave como segredo do servidor, na variável **`BRAPI_TOKEN`** (no
   cluster, o Secret do app; **nunca** no front, no repositório ou numa URL).
3. Libere a saída HTTPS para **`brapi.dev`** (a API) e **`icons.brapi.dev`** (os
   arquivos dos logos) no `egress` do deploy.

Sem `BRAPI_TOKEN` a funcionalidade fica **desligada**: o servidor não fala com a
brapi, todo ativo novo usa o ícone de iniciais e nada dá erro. O que já estiver
guardado no banco continua sendo servido.

## De onde vem o logo

A [brapi](https://brapi.dev) (API de dados do mercado brasileiro). Medido em
26/11/2026:

1. `GET https://brapi.dev/api/v2/stocks/quote?symbols=ITSA4`, com
   `Authorization: Bearer <token>`, devolve `results[].data.logourl`
   (`https://icons.brapi.dev/icons/ITSA4.svg`). O token vai **só no cabeçalho**,
   nunca na URL (a brapi desaconselha `?token=` porque a URL cai em log). A rota
   antiga `/api/quote/{ticker}` devolve o mesmo campo, mas a brapi manda usar a
   v2 em integração nova (e sinaliza a v1 com `Link: rel="deprecation"`).
2. `GET <logourl>`, **sem token**: `icons.brapi.dev` é um host estático que
   serve `image/svg+xml` (24 logos de ações e FIIs medidos: de 263 B a 2,4 kB,
   56×56, só `<path>`, `<linearGradient>` e `<stop>`).
3. Sem token a API responde `401` com
   `{"error":true,"message":"Token de autenticação não fornecido","code":"MISSING_TOKEN"}`.
   Só PETR4, MGLU3, VALE3 e ITUB4 respondem sem token (sandbox, 20 pedidos por
   minuto por IP); o Prumo não usa isso.

Só o que está na B3 (`listed_on_b3`: ações, FIIs, ETFs, BDRs) vai à brapi. Ações
e REITs dos EUA, cripto de verdade, títulos do Tesouro e renda fixa por valor
**não** buscam logo: a brapi só cobre a B3.

### Limites do plano gratuito e como o Prumo os respeita

Segundo a página de preços (26/11/2026): **15 mil pedidos por mês, 1 ticker por
pedido e 1 pedido por vez** (pedidos em paralelo podem receber 429 mesmo com
cota). Por isso:

- cada símbolo é buscado **uma vez** e guardado (`asset_icons`); só é conferido
  de novo depois de **30 dias** (logo achado ou "a brapi não tem") ou **6 horas**
  depois de uma falha. Uma carteira de 50 ativos gasta 50 pedidos, uma vez;
- as buscas de uma réplica passam por uma fila de uma (um pedido por vez);
- duas réplicas não buscam o mesmo símbolo: a reserva é feita no banco, numa
  transação curta sob um advisory lock, e **nenhuma conexão fica presa durante a
  ida à rede** (o pool tem 4). Reserva de réplica que caiu vence em 15 minutos;
- `429`, token recusado (`401`) ou plano sem o recurso (`403`) **param** a tarefa
  (pelo `Retry-After` da brapi, ou 6 horas); três falhas de rede seguidas
  encerram a rodada. Falha de revalidação **não apaga** o logo que já existia;
- a tarefa roda no arranque e de hora em hora; um ativo novo (cadastro, mudança
  de mercado ou importação da B3) dispara a busca do próprio símbolo.

## Segurança: o SVG é de terceiro e sai do nosso domínio

Um SVG pode carregar `<script>`, `onload=`, `<foreignObject>` com HTML e
referências a recursos de fora. Servido pelo domínio do Prumo, seria código de
terceiro na nossa origem. **Escolha: sanear no servidor e servir com cabeçalhos
que desligam tudo (opção "b"), em vez de rasterizar para PNG (opção "a").**

1. **Saneamento por lista de permitidos** (`domain/icon.rs`). O SVG é lido e
   **escrito de novo**: só passam formas e gradientes (`svg g defs path rect
   circle ellipse line polyline polygon linearGradient radialGradient stop
   clipPath`) e atributos de desenho (`d`, `fill`, `stroke`, `transform`,
   `viewBox`…). O namespace é escrito pelo Prumo. Recusam o logo inteiro (que
   vira "sem logo"): elemento fora da lista (`script`, `foreignObject`, `image`,
   `use`, `a`, `style`, `animate`, `set`, `text`…), atributo `on*`, valor com
   `javascript:`, `data:`, `http(s):`, `//`, `expression(`, `@import`, `url(` que
   não aponte para `#id` do próprio arquivo, `href` que não seja `#`, caracteres
   fora do alfabeto de `d` e `transform`, `DOCTYPE` (XXE e bomba de entidades),
   texto solto, CDATA, namespace trocado, mais de 128 KiB, 32 níveis ou 2 mil
   elementos. Atributo inofensivo desconhecido (`class`, `inkscape:*`) é
   descartado em silêncio; `<title>`, `<desc>` e `<metadata>` saem com o que há
   dentro. Só o resultado vai para o banco.
2. **Resposta endurecida** (`http/icons.rs`): `Content-Security-Policy:
   default-src 'none'; style-src 'unsafe-inline'; sandbox`, `X-Content-Type-Options:
   nosniff`, `Content-Disposition: inline`, `Cross-Origin-Resource-Policy:
   same-origin`, `Cache-Control: private`. A tela só usa o logo em `<img>`, onde
   o navegador não executa script de SVG de qualquer forma. A CSP da página
   (`img-src 'self' data:`) não muda.
3. **A URL vem de fora, então é conferida**: o `logourl` só vale se começar por
   `https://icons.brapi.dev/`, terminar em `.svg` e não tiver `..`, `?` ou `#`;
   o cliente HTTP do download **não segue redirecionamento**; o tipo precisa ser
   `image/svg+xml` e o corpo tem teto de 128 KiB (lido aos poucos, sem baixar o
   que passa disso). O token **nunca** vai para `icons.brapi.dev`.
4. O token nunca aparece em log, erro, resposta ou `Debug` (`Secret`, igual ao
   token do Telegram); os erros do cliente não citam URL nem corpo da resposta.

Os testes cobrem: um SVG malicioso fictício (`tests/fixtures/logo_malicioso.svg`,
com `onload`, `<script>` e `<foreignObject>`) que nunca chega ao banco nem à
resposta; dezenas de variações de ataque recusadas (as principais, cada uma pelo motivo certo); proptest
(a saída aceita nunca tem `<script`, `javascript:`, `DOCTYPE` ou `foreignObject`,
é estável ao sanear de novo, e bytes quaisquer nunca causam pânico); a rota
(autenticação, cabeçalhos, `ETag`/304, 404). Os 24 logos reais medidos passam
**sem mudar um byte**.

### Por que não rasterizar (opção "a")

Rasterizar (por exemplo com `resvg`) serviria PNG e dispensaria o saneamento,
mas o servidor passaria a **interpretar** o SVG inteiro de um terceiro (parser
completo, fontes, filtros, referências), com uma árvore grande de dependências
para desenhar umas 20 formas, e o PNG fixo borra em telas de alta densidade.
A lista de permitidos recusa por construção o que a rasterização teria de
tratar, não adiciona dependência (o `quick-xml` já estava) e entrega um SVG
pequeno e nítido. Não medi o peso do `resvg` nem rodei `cargo audit` nele: a
decisão veio antes disso. Se um dia um logo legítimo precisar de recursos fora da
lista, o caminho é rasterizar só esse caso.

## Licença e termos da brapi

- O plano **gratuito** é descrito pela brapi como para "testing, personal, and
  academic projects" (página de preços de 26/11/2026); planos pagos, para
  produtos em produção. O Prumo é uso pessoal de uma pessoa só.
- A página de preços lista "Logos das empresas" entre os dados, **sem dizer em
  quais planos**, e as chamadas de cotação do plano gratuito devolvem `logourl`.
  Se a brapi deixar de incluir o logo no plano gratuito, a API responde `403` e
  a tarefa para sozinha (iniciais no lugar).
- **Não consegui abrir uma página de termos de uso** (nenhum endereço público
  conhecido respondeu em 26/11/2026). Antes de depender disso, leia os termos
  no painel da brapi. Os logos são **marcas de terceiros**, de propriedade das
  empresas: o Prumo os mostra só à pessoa dona da carteira, dentro do app, e
  **não os guarda no repositório** (os testes usam um logo fictício).
- Os termos pedem o token só no servidor, nunca no front: é o que o Prumo faz.
  Não há atribuição obrigatória conhecida; se a brapi pedir, ela entra aqui.

## O que ficou de fora

- Logo de ativo dos EUA: a brapi não cobre; ficam as iniciais.
- Logo de título do Tesouro e de cripto: ícone próprio (desenhado no front).
- Revalidação sob demanda: só a rodada de hora em hora (e a busca de ativo novo).
