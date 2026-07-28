-- 03-arith: `+` and `-` in the backend, and the two things they refuse.
--
-- Ports: addition_within_one_currency_is_exact,
-- addition_is_exact_at_one_unit_of_the_eighteenth_decimal,
-- addition_across_currencies_is_refused_at_runtime, addition_past_the_domain_top_is_refused.
--
-- These delegate to kamu_money_core's add_units/sub_units -- the same kernel Money::checked_add
-- runs. No numeric, no base-10000 limbs, no scale to lose.
\pset pager off
\pset footer off
\pset format unaligned
\pset tuples_only on
\pset null '<NULL>'
\set VERBOSITY terse
SET client_min_messages = error;
CREATE EXTENSION IF NOT EXISTS kmoney;

\echo -- addition_within_one_currency_is_exact
SELECT ('USD 10.50'::kmoney + 'USD 0.50'::kmoney)::text || ' | '
    || ('USD 10.50'::kmoney - 'USD 0.50'::kmoney)::text;

\echo -- addition_is_exact_at_one_unit_of_the_eighteenth_decimal
-- Exactness at the smallest representable step, where a float or a rounded numeric has already
-- given up.
SELECT ('IDR 999999999999999999.999999999999999998'::kmoney
      + 'IDR 0.000000000000000001'::kmoney)::text;

\echo -- addition_across_currencies_is_refused_at_runtime
SELECT ('USD 1.00'::kmoney + 'IDR 1.00'::kmoney)::text;

\echo -- addition_past_the_domain_top_is_refused
-- Never a wrap, never a saturation.
SELECT ('IDR 999999999999999999.999999999999999999'::kmoney
      + 'IDR 0.000000000000000001'::kmoney)::text;

\echo == CASE COMPLETE: 03-arith ==
