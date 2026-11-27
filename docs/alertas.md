# Alertas pelo Telegram

Avisos fora do app, num **resumo diário**: uma mensagem por dia, só quando há
algo a dizer. Nunca uma mensagem por evento (o risco do roteiro, § 3.11, é
spam). A tela é a seção **Ajustes › Alertas**; o contrato está em
[api.md](api.md#alertas).

## O que o Prumo precisa de você (uma vez)

O Prumo **não cria bot nem conta** no Telegram. Quem cria é você, no
[@BotFather](https://t.me/BotFather), e entrega o token ao servidor.

1. No Telegram, abra o **@BotFather** e mande `/newbot`.
2. Escolha um nome (aparece no chat) e um usuário que termine em `bot`
   (por exemplo `prumo_do_fulano_bot`).
3. O BotFather responde com o **token**, no formato `123456789:AA...`. Ele dá
   acesso total ao bot: trate como senha, não cole em chat, issue nem commit.
4. Guarde o token como segredo do servidor, na variável **`TELEGRAM_BOT_TOKEN`**
   (no cluster, pelo mesmo caminho dos outros segredos do Prumo). Reinicie o
   Prumo.
5. Libere a saída HTTPS para **`api.telegram.org`** (`egress` do deploy).
6. Abra **Ajustes › Alertas** e toque em **Vincular o Telegram**.

Opcional, no BotFather: `/setdescription` e `/setuserpic` do bot, e
`/setprivacy` não importa (o Prumo só conversa em chat privado).

Sem `TELEGRAM_BOT_TOKEN`, a funcionalidade fica **desligada**: o servidor não
chama o Telegram, a tela diz "os alertas estão desligados neste servidor" e
nada dá erro.

## Vincular o seu chat (sem procurar chat id)

1. Em **Ajustes › Alertas**, toque em **Vincular o Telegram**. A tela mostra um
   código de 10 caracteres e o link `t.me/<seu_bot>?start=<código>`.
2. Toque no link (ele abre o bot com o código pronto) e **Iniciar**; ou abra o
   bot e mande `/start <código>` à mão.
3. O Prumo lê a mensagem, descobre o seu `chat_id` e liga o chat à sua conta.
   A tela confere sozinha de 3 em 3 segundos e avisa quando fecha. O bot
   responde no chat.

O código vale **10 minutos** e **uma vez só**; gerar outro invalida o anterior.
Sem o código certo, quem achar o bot não consegue ligar o chat à sua conta.

Para desvincular: **Desvincular** na tela, ou `/stop` no chat com o bot. Se o
Telegram recusar um envio (você bloqueou o bot), o Prumo desvincula sozinho.

### Como o Prumo lê o Telegram

Por **polling** (`getUpdates` com `offset`), e não por webhook: o Prumo fica
atrás do Cloudflare Access, que bloquearia as chamadas do Telegram. O `offset`
fica no banco (`telegram_poll_state`), então reiniciar não repete nem perde
mensagem. Só o chat **privado** com o bot conta; grupos são ignorados.

## O que avisa

Cada tipo liga e desliga sozinho na tela. Um aviso "de transição" só vale
**quando passa a valer**: o que já valia na apuração anterior não se repete; se
sair da situação e voltar, avisa de novo. Ao ligar um tipo (ou mudar o limite
dele), a primeira apuração só **guarda** o que já vale, sem avisar — senão ligar
despejaria tudo de uma vez.

| Tipo | Quando entra no resumo | Limite |
|---|---|---|
| `income_tomorrow` | provento lançado em Renda com pagamento **amanhã** (somado por pagador, líquido) | — |
| `class_drift` | uma classe passou a ficar a X p.p. ou mais da meta, para cima ou para baixo | X, padrão 5 p.p. |
| `price_below_ceiling` | um ativo com teto de preço voltou a custar o teto ou menos (voltou a ser comprável no aporte); o teto é o de hoje, o valor próprio ou o do método escolhido, e método sem valor não avisa | — |
| `objective_achieved` | um objetivo seu passou a constar como atingido (`objectives.achieved_at`) | — |
| `tax_exemption` | as vendas de ações do mês chegaram a X% do limite de isenção, e de novo quando o passaram | X% do limite, padrão 80% |
| `health_warning` | uma regra de [Saúde da carteira](saude.md) passou a avisar (regra + ativo, setor ou classe) | os da própria Saúde; entram também as de renda (provento em todo mês e pagador acima do limite). Sem valores, o pagador sai só com o percentual |

Todos vêm ligados. O resumo sai **uma vez por dia**, no
horário que você escolher (padrão **08:00, horário de Brasília**), e só se
houver algo. Se o servidor estava fora do ar no horário, o resumo sai quando
voltar, naquele mesmo dia. Se o Telegram falhar, o Prumo tenta de novo a cada
minuto até conseguir (um pedido de espera `429` é respeitado, até 30 s).

**Enviar teste agora** manda uma mensagem curta, sem dado da carteira, que
confirma o canal e diz a hora do resumo. Um teste a cada 10 segundos, no
máximo.

### Isentômetro

É a mesma apuração da tela Impostos (`GET /tax/months`): vendas de ações em
operação comum no **mês de hoje** contra o limite de isenção do ano
(`domain/tax/rules`). O resumo avisa duas vezes por mês, no máximo: ao chegar a
X% do limite (padrão 80%) e, de novo, ao passar dele (a partir daí o lucro das
ações vendidas no mês é tributado). Vender mais depois disso não repete o aviso.
O instrumento fiscal de cada ativo é o da tela Impostos (`assets.tax_instrument`).

Sem regra de IR cadastrada para o ano de hoje, o tipo fica **indisponível**
(`available: false`), desligado e sem poder ser ligado, e o resumo não o
apura; não há erro, e o Prumo nunca usa a regra de outro ano. Se o livro não
fecha para o IR (a tela Impostos mostra o motivo), o isentômetro também não diz
nada naquele dia, e o resto do resumo sai normal. Com "sem valores", só o
percentual do limite vai ao Telegram.

### Objetivo atingido

O alerta só lê `objectives.achieved_at`, que a tarefa diária dos objetivos
preenche para todas as pessoas (e a tela, ao abrir). Um objetivo que a tarefa
marcou durante a noite entra no resumo da manhã.

## Valores no resumo

O resumo atravessa os servidores do **Telegram**. Conversa com bot **não** é
criptografada de ponta a ponta: o Telegram consegue ler. Por isso:

- **Com valores** (padrão): o resumo traz reais — o líquido do provento, o
  preço e o teto, o valor do objetivo, o aporte que traz uma regra de saúde de
  volta ao limite.
- **Sem valores** (opção "Sem valores no resumo"): só **nomes e percentuais**.
  Nenhum `R$` sai: o provento diz só quem paga amanhã, o preço diz que voltou
  ao teto sem dizer qual, o objetivo diz que foi atingido, a saúde diz o
  percentual e o limite. Há teste de propriedade garantindo que nenhuma
  mensagem "sem valores" contém `R$`.

Nomes de ativos e percentuais sempre vão (sem eles o aviso não diz nada).
O olho de ocultar valores da barra é outra coisa: esconde a **tela** neste
navegador; a escolha "sem valores" vale para o que **sai do servidor**. A tela
mostra um exemplo do resumo (com dados fictícios) que segue as duas.

O texto do resumo, do teste e das respostas do bot (`/start`, `/stop`, código
inválido ou vencido) é o que você lê no Telegram, que é UTF-8: vai em português
correto, **com acento**, como a tela. A regra de texto sem acento existe por
causa do **log**: log e erro HTTP do servidor (`title` e `detail`, mensagens de
log) seguem sem acento. O token nunca aparece em log, em erro nem em resposta da API: o erro de rede é
limpo da URL (o token mora nela, na Bot API) e a `Config` o imprime como
`Secret(***)`.

## Várias réplicas

Duas tarefas em segundo plano disputam **advisory locks do Postgres**, como a
tarefa diária de desempenho:

- leitura do Telegram: `0x50_52_55_4d_4f_27`, uma réplica por rodada de
  `getUpdates` (duas leriam as mesmas mensagens e o Telegram devolveria `409`);
- resumo diário: `0x50_52_55_4d_4f_17`, uma réplica por vez apura e envia. A
  apuração grava o dia (`alert_settings.last_run_on`) antes de soltar o lock,
  então quem pega o lock depois não encontra mais ninguém devido: o resumo não
  duplica.

Se a réplica que tinha o lock cair, o Postgres o solta e outra assume.

## Dados guardados (migração 0017)

| Tabela | O quê |
|---|---|
| `alert_settings` | por pessoa: `telegram_chat_id`, horário, "sem valores", último dia apurado |
| `alert_types` | por pessoa e tipo: ligado, limite, e se já teve a primeira apuração |
| `alert_seen` | o que já valia na última apuração de cada tipo de transição |
| `alert_link_codes` | códigos de vinculação (10 min, uso único) |
| `telegram_poll_state` | o `offset` do `getUpdates` |

## Se não chega

- A tela diz "desligados neste servidor": falta `TELEGRAM_BOT_TOKEN`.
- "Enviar teste" dá 502: o token está errado, o bot foi apagado, ou o servidor
  não alcança `api.telegram.org` (confira o `egress`).
- Vinculou e nada chega: o horário ainda não passou, ou não há nada novo — o
  resumo é silencioso por desenho. Use "Enviar teste agora" para separar canal
  de conteúdo.
- O bot não responde ao `/start`: o servidor lê o Telegram a cada rodada de até
  25 s; se o log mostra `409`, há outro processo lendo o mesmo bot (outro
  ambiente com o mesmo token). Cada ambiente precisa do **seu** bot.
