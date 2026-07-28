-- 11-compare: equality is TOTAL, ordering REFUSES across currencies, and there is no way out to
-- `numeric`.
--
-- Ports: equality_is_currency_aware_and_never_raises, ordering_refuses_cross_currency,
-- there_is_no_cast_to_numeric.
\pset pager off
\pset footer off
\pset format unaligned
\pset tuples_only on
\pset null '<NULL>'
\set VERBOSITY terse
SET client_min_messages = error;
CREATE EXTENSION IF NOT EXISTS kmoney;

\echo -- equality_is_currency_aware_and_never_raises
CREATE TEMP TABLE cmp (amount kmoney);
INSERT INTO cmp VALUES ('USD 1.00'), ('USD 2.00'), ('IDR 1.00'), ('USD 1.00');
-- `=` never raises across currencies, so it is safe as a predicate everywhere -- including on a
-- column holding several. Ordering within ONE currency filters normally, which is all a wallet
-- whose columns are typmod-pinned ever asks of it.
SELECT 'cross_eq=' || ('USD 1.00'::kmoney = 'IDR 1.00'::kmoney)
    || ' same_eq=' || ('USD 1.00'::kmoney = 'USD 1.00'::kmoney)
    || ' usd_ones=' || (SELECT count(*) FROM cmp WHERE amount = 'USD 1.00'::kmoney)
    || ' gt_within_one_currency=' || ('USD 2.00'::kmoney > 'USD 1.00'::kmoney);

\echo -- ordering_refuses_cross_currency
-- Comparing < / > across currencies would order by ISO numeric code, so `WHERE amount > 'USD
-- 1.00'` on a column that happens to hold several could report a tiny foreign amount as GREATER
-- THAN a dollar. Ordering therefore errors exactly like `+`; equality stays total.
SELECT 'IDR 1.00'::kmoney > 'USD 1.00'::kmoney;

\echo -- there_is_no_cast_to_numeric
-- Load-bearing rather than an omission: a bare `numeric` puts every silently-rounding
-- PostgreSQL operator back in scope (E9). The text form is the egress.
SELECT ('USD 1.00'::kmoney)::numeric::text;

\echo == CASE COMPLETE: 11-compare ==
