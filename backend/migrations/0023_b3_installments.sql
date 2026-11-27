-- Parcelas de um provento: a B3 publica uma linha por parcela, com o mesmo
-- ISIN, rotulo, data com e valor, e so a data de pagamento muda (o dividendo
-- de 12/2025 da WEGE3 em tres pagamentos anuais, os JCP da B3SA3 em quatro).
-- A chave antiga juntava as parcelas numa so e o provento automatico contava
-- uma parcela de varias. A data de pagamento entra na chave; nula ("a
-- definir") e um valor como outro qualquer (`nulls not distinct`).
alter table b3_cash_announcements drop constraint b3_cash_announcements_pkey;
alter table b3_cash_announcements add constraint b3_cash_announcements_key
    unique nulls not distinct (isin, label, record_date, rate, pay_date);
