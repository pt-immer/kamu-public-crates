-- 08-typmod: `kmoney('IDR')` pins a column to one currency -- and does NOT reach operators.
--
-- Ports: a_typmod_column_round_trips_its_currency, a_typmod_column_refuses_the_wrong_currency,
-- an_unpinned_column_still_accepts_every_currency, a_typmod_of_an_unknown_currency_is_refused,
-- two_type_modifiers_are_refused,
-- typmod_does_not_reach_operators_so_the_value_check_still_fires.
\pset pager off
\pset footer off
\pset format unaligned
\pset tuples_only on
\pset null '<NULL>'
\set VERBOSITY terse
SET client_min_messages = error;
CREATE EXTENSION IF NOT EXISTS kmoney;

\echo -- a_typmod_column_round_trips_its_currency
CREATE TEMP TABLE pinned (amount kmoney('IDR'));
INSERT INTO pinned VALUES ('IDR 16000.00');
SELECT amount::text FROM pinned;
-- format_type is what \d and pg_dump read; typmod_out must round-trip or a dump does not restore.
SELECT format_type(atttypid, atttypmod) FROM pg_attribute
 WHERE attrelid = 'pinned'::regclass AND attname = 'amount';

\echo -- a_typmod_column_refuses_the_wrong_currency
-- THE POINT OF TYPMOD: refused at INSERT, before it is stored.
CREATE TEMP TABLE pinned_reject (amount kmoney('IDR'));
INSERT INTO pinned_reject VALUES ('USD 1.00');

\echo -- an_unpinned_column_still_accepts_every_currency
-- typmod -1 is "no modifier", not "no currency".
CREATE TEMP TABLE unpinned (amount kmoney);
INSERT INTO unpinned VALUES ('USD 1.00'), ('IDR 16000.00');
SELECT 'rows=' || count(*) FROM unpinned;

\echo -- a_typmod_of_an_unknown_currency_is_refused
CREATE TEMP TABLE bad_typmod (amount kmoney('ZWL'));

\echo -- two_type_modifiers_are_refused
CREATE TEMP TABLE two_mods (amount kmoney('IDR', 'USD'));

\echo -- typmod_does_not_reach_operators_so_the_value_check_still_fires
-- Two differently pinned columns still meet as bare kmoney + kmoney, so the
-- refusal comes from the value-carried currency code rather than from the column types. Both
-- mechanisms are required.
CREATE TEMP TABLE lhs (amount kmoney('IDR'));
CREATE TEMP TABLE rhs (amount kmoney('USD'));
INSERT INTO lhs VALUES ('IDR 1.00');
INSERT INTO rhs VALUES ('USD 1.00');
SELECT (l.amount + r.amount)::text FROM lhs l, rhs r;

\echo == CASE COMPLETE: 08-typmod ==
