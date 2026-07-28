-- 06-div: division returns the residue BESIDE the quotient, and the identity holds under every
-- rounding mode.
--
-- Ports: division_returns_the_residue_beside_the_quotient,
-- the_division_identity_holds_for_every_rounding_mode, division_refuses_an_unknown_rounding_mode.
--
-- The seven modes are written out as seven statements rather than driven from an array. A
-- correlated set-returning call inside a scalar subquery is a planner shape, and this file is
-- supposed to measure kmoney on two engines -- not to discover which planner inlines what.
\pset pager off
\pset footer off
\pset format unaligned
\pset tuples_only on
\pset null '<NULL>'
\set VERBOSITY terse
SET client_min_messages = error;
CREATE EXTENSION IF NOT EXISTS kmoney;

\echo -- division_returns_the_residue_beside_the_quotient
-- SQL cannot force the caller to look at the residue -- that guarantee does not cross the
-- boundary -- but it cannot be produced without being returned either.
SELECT quotient::text || ' | ' || residue::text FROM kmoney_div('USD 10.00', 3, 'toward_zero');

\echo -- the_division_identity_holds_for_every_rounding_mode
-- quotient * 3 + residue == amount, exactly, for every mode. This is the identity the residue
-- exists to preserve, checked in SQL rather than assumed from the Rust tests.
SELECT 'half_even=' || ((SELECT kmoney_sum(VARIADIC array_agg(q)) FROM (
              SELECT quotient AS q FROM kmoney_div('USD 10.00', 3, 'half_even')
    UNION ALL SELECT quotient      FROM kmoney_div('USD 10.00', 3, 'half_even')
    UNION ALL SELECT quotient      FROM kmoney_div('USD 10.00', 3, 'half_even')
    UNION ALL SELECT residue       FROM kmoney_div('USD 10.00', 3, 'half_even')) parts)::text
    = 'USD 10.00');
SELECT 'half_away_from_zero=' || ((SELECT kmoney_sum(VARIADIC array_agg(q)) FROM (
              SELECT quotient AS q FROM kmoney_div('USD 10.00', 3, 'half_away_from_zero')
    UNION ALL SELECT quotient      FROM kmoney_div('USD 10.00', 3, 'half_away_from_zero')
    UNION ALL SELECT quotient      FROM kmoney_div('USD 10.00', 3, 'half_away_from_zero')
    UNION ALL SELECT residue       FROM kmoney_div('USD 10.00', 3, 'half_away_from_zero')) parts)::text
    = 'USD 10.00');
SELECT 'half_toward_zero=' || ((SELECT kmoney_sum(VARIADIC array_agg(q)) FROM (
              SELECT quotient AS q FROM kmoney_div('USD 10.00', 3, 'half_toward_zero')
    UNION ALL SELECT quotient      FROM kmoney_div('USD 10.00', 3, 'half_toward_zero')
    UNION ALL SELECT quotient      FROM kmoney_div('USD 10.00', 3, 'half_toward_zero')
    UNION ALL SELECT residue       FROM kmoney_div('USD 10.00', 3, 'half_toward_zero')) parts)::text
    = 'USD 10.00');
SELECT 'toward_zero=' || ((SELECT kmoney_sum(VARIADIC array_agg(q)) FROM (
              SELECT quotient AS q FROM kmoney_div('USD 10.00', 3, 'toward_zero')
    UNION ALL SELECT quotient      FROM kmoney_div('USD 10.00', 3, 'toward_zero')
    UNION ALL SELECT quotient      FROM kmoney_div('USD 10.00', 3, 'toward_zero')
    UNION ALL SELECT residue       FROM kmoney_div('USD 10.00', 3, 'toward_zero')) parts)::text
    = 'USD 10.00');
SELECT 'away_from_zero=' || ((SELECT kmoney_sum(VARIADIC array_agg(q)) FROM (
              SELECT quotient AS q FROM kmoney_div('USD 10.00', 3, 'away_from_zero')
    UNION ALL SELECT quotient      FROM kmoney_div('USD 10.00', 3, 'away_from_zero')
    UNION ALL SELECT quotient      FROM kmoney_div('USD 10.00', 3, 'away_from_zero')
    UNION ALL SELECT residue       FROM kmoney_div('USD 10.00', 3, 'away_from_zero')) parts)::text
    = 'USD 10.00');
SELECT 'floor=' || ((SELECT kmoney_sum(VARIADIC array_agg(q)) FROM (
              SELECT quotient AS q FROM kmoney_div('USD 10.00', 3, 'floor')
    UNION ALL SELECT quotient      FROM kmoney_div('USD 10.00', 3, 'floor')
    UNION ALL SELECT quotient      FROM kmoney_div('USD 10.00', 3, 'floor')
    UNION ALL SELECT residue       FROM kmoney_div('USD 10.00', 3, 'floor')) parts)::text
    = 'USD 10.00');
SELECT 'ceil=' || ((SELECT kmoney_sum(VARIADIC array_agg(q)) FROM (
              SELECT quotient AS q FROM kmoney_div('USD 10.00', 3, 'ceil')
    UNION ALL SELECT quotient      FROM kmoney_div('USD 10.00', 3, 'ceil')
    UNION ALL SELECT quotient      FROM kmoney_div('USD 10.00', 3, 'ceil')
    UNION ALL SELECT residue       FROM kmoney_div('USD 10.00', 3, 'ceil')) parts)::text
    = 'USD 10.00');

\echo -- division_refuses_an_unknown_rounding_mode
-- No default rounding mode in SQL either: a default is a decision made by whoever wrote the
-- library rather than by whoever owns the money.
SELECT quotient::text FROM kmoney_div('USD 10.00', 3, 'bankers');

\echo == CASE COMPLETE: 06-div ==
