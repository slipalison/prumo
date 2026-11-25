-- Hora do negócio que formou o preço, como o provedor informa. `fetched_at`
-- continua sendo quando o Prumo gravou. Com o streamer, o mesmo símbolo chega
-- por dois caminhos (streamer e `chart`); o preço de negócio mais recente
-- vence, e a tela mostra de quando o preço é — na B3, 15 minutos atrás.
alter table quotes add column traded_at timestamptz;
