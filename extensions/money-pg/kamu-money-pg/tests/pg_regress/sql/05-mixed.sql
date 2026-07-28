-- 05-mixed: `kmoney_mixed` -- a column that holds several currencies, stores and filters, and
-- cannot be added.
--
-- Ports: sum_on_a_mixed_column_fails_at_plan_time,
-- a_mixed_column_equality_is_currency_aware_and_never_raises, a_mixed_column_cannot_be_ordered,
-- addition_on_the_mixed_type_does_not_exist_either,
-- a_mixed_column_stores_several_currencies_side_by_side,
-- the_conversion_out_of_mixed_proves_the_currency,
-- the_conversion_out_of_mixed_refuses_the_wrong_currency.
--
-- NOTE ON ORDERING vs THE RUST SUITE. In the backend a raised ERROR longjmps out of SPI and
-- aborts the whole test transaction, so each refusal needed its own #[pg_test]. Here every
-- statement is its own implicit transaction, so a refusal and the successes around it live in
-- one file. Same assertions, fewer sessions.
\pset pager off
\pset footer off
\pset format unaligned
\pset tuples_only on
\pset null '<NULL>'
\set VERBOSITY terse
SET client_min_messages = error;
CREATE EXTENSION IF NOT EXISTS kmoney;

\echo -- a_mixed_column_equality_is_currency_aware_and_never_raises
CREATE TEMP TABLE mixed_eq (amount kmoney_mixed);
INSERT INTO mixed_eq VALUES ('USD 1.00'), ('IDR 1.00'), ('USD 1.00'), ('USD 2.00');
-- Equality is TOTAL: it filters a mixed column without raising. Same number, different
-- currency, different money.
SELECT 'cross_currency_eq=' || ('USD 1.00'::kmoney_mixed = 'IDR 1.00'::kmoney_mixed)
    || ' usd_ones=' || (SELECT count(*) FROM mixed_eq WHERE amount = 'USD 1.00'::kmoney_mixed);

\echo -- a_mixed_column_stores_several_currencies_side_by_side
CREATE TEMP TABLE mixed_ok (amount kmoney_mixed);
INSERT INTO mixed_ok VALUES ('USD 1.00'), ('IDR 16000.00'), ('JPY 150');
SELECT string_agg(amount::text, ', ' ORDER BY amount::text) FROM mixed_ok;

\echo -- the_conversion_out_of_mixed_proves_the_currency
-- The SQL twin of proving a value into a typed Money<C> before it may be added.
SELECT kmoney_from_mixed('USD 2.50'::kmoney_mixed, 'USD')::text;

\echo -- the_conversion_out_of_mixed_refuses_the_wrong_currency
SELECT kmoney_from_mixed('IDR 2.50'::kmoney_mixed, 'USD')::text;

\echo -- a_mixed_column_cannot_be_ordered
-- No B-tree opclass, deliberately: ordering a column holding several currencies would sort by
-- (currency, units) while reading as though it sorted by value.
SELECT 'USD 1.00'::kmoney_mixed < 'USD 2.00'::kmoney_mixed;

\echo -- addition_on_the_mixed_type_does_not_exist_either
SELECT ('USD 1.00'::kmoney_mixed + 'USD 1.00'::kmoney_mixed)::text;

\echo -- sum_on_a_mixed_column_fails_at_plan_time
-- Not a check that runs: an operation that never existed. This fails when the statement is
-- PLANNED, before any row is read -- the SQL analogue of `Add` being absent on an untyped money.
CREATE TEMP TABLE payments (amount kmoney_mixed);
INSERT INTO payments VALUES ('USD 1.00'), ('IDR 16000.00');
SELECT sum(amount)::text FROM payments;

\echo == CASE COMPLETE: 05-mixed ==
