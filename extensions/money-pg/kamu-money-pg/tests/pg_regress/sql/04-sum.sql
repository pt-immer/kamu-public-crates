-- 04-sum: `kmoney_sum` and the `sum(kmoney)` aggregate over a wide transition state.
--
-- `sum(kmoney)` uses `UnitSum` -- 32 bytes of `I256` plus the currency code -- so no reachable
-- partial sum can leave the transition state's domain.
--
-- Ports: kmoney_sum_adds_an_explicit_list_within_one_currency,
-- kmoney_sum_is_order_independent_across_a_domain_edge_transient, kmoney_sum_of_nothing_is_null,
-- kmoney_sum_rejects_a_mixed_currency_argument, kmoney_sum_rejects_a_total_that_leaves_the_domain,
-- the_sum_aggregate_totals_a_column, the_sum_aggregate_of_nothing_is_null,
-- the_sum_aggregate_agrees_with_the_variadic_form,
-- the_sum_aggregate_is_plan_independent_across_a_domain_edge_transient,
-- the_sum_aggregate_combines_an_empty_partial,
-- the_sum_aggregate_rejects_a_forged_transition_state,
-- the_sum_aggregate_refuses_a_mixed_currency_column.
\pset pager off
\pset footer off
\pset format unaligned
\pset tuples_only on
\pset null '<NULL>'
\set VERBOSITY terse
SET client_min_messages = error;
CREATE EXTENSION IF NOT EXISTS kmoney;

\echo -- kmoney_sum_adds_an_explicit_list_within_one_currency
SELECT kmoney_sum('USD 10.50', 'USD 0.25', 'USD 0.25')::text;

\echo -- kmoney_sum_is_order_independent_across_a_domain_edge_transient
-- MAX + MAX transiently leaves the domain while the final total remains inside it.
-- Wide accumulation makes the result independent of argument order.
SELECT kmoney_sum('USD 999999999999999999.999999999999999999',
                  'USD 999999999999999999.999999999999999999',
                  'USD -999999999999999999.999999999999999999')::text || ' | '
    || kmoney_sum('USD 999999999999999999.999999999999999999',
                  'USD -999999999999999999.999999999999999999',
                  'USD 999999999999999999.999999999999999999')::text || ' | '
    || kmoney_sum('USD -999999999999999999.999999999999999999',
                  'USD 999999999999999999.999999999999999999',
                  'USD 999999999999999999.999999999999999999')::text;

\echo -- kmoney_sum_of_nothing_is_null
-- An explicit empty array, never a bare kmoney_sum(): PostgreSQL will NOT resolve a
-- zero-argument call to a VARIADIC function. No currency to carry means NULL, never a
-- currencyless zero.
SELECT 'empty_sum_is_null=' || (kmoney_sum(VARIADIC ARRAY[]::kmoney[]) IS NULL);

\echo -- kmoney_sum_rejects_a_mixed_currency_argument
SELECT kmoney_sum('USD 1.00', 'IDR 1.00')::text;

\echo -- kmoney_sum_rejects_a_total_that_leaves_the_domain
-- The domain check fires once, at the end, on the true total.
SELECT kmoney_sum('USD 999999999999999999.999999999999999999',
                  'USD 0.000000000000000001')::text;

\echo -- the_sum_aggregate_totals_a_column
-- The aggregate uses a wide transition state.
CREATE TEMP TABLE ledger (amount kmoney);
INSERT INTO ledger VALUES ('USD 10.50'), ('USD 0.25'), ('USD 0.25');
SELECT sum(amount)::text FROM ledger;

\echo -- the_sum_aggregate_of_nothing_is_null
-- A NULL row is skipped, exactly as sum() skips NULLs everywhere else; a group with no rows at
-- all totals to NULL, never a currencyless zero -- there is no currency to carry.
INSERT INTO ledger VALUES (NULL);
SELECT sum(amount)::text FROM ledger;
SELECT 'empty_group_is_null=' || (sum(amount) IS NULL) FROM ledger WHERE false;

\echo -- the_sum_aggregate_agrees_with_the_variadic_form
-- One kernel, two entry points. If these ever disagreed, one of them would be inventing money.
SELECT 'aggregate_equals_variadic=' || (sum(amount) = kmoney_sum(VARIADIC array_agg(amount)))
    FROM ledger;

\echo -- the_sum_aggregate_is_plan_independent_across_a_domain_edge_transient
-- Driving the transition and combine functions by hand simulates two parallel workers
-- DETERMINISTICALLY -- waiting for the planner to choose a parallel plan would make this a
-- statement about the planner, and YugabyteDB and stock PostgreSQL need not plan alike. One
-- worker's partial (MAX + MAX) has LEFT the domain; the other's (-MAX) brings it back. The
-- removed narrow state failed on that transient, and failed differently depending on which side
-- arrived first, which is exactly what made the total a property of the plan.
SELECT kmoney_sum_final(kmoney_sum_combine(
           kmoney_sum_accum(kmoney_sum_accum(NULL::bytea,
               'USD 999999999999999999.999999999999999999'::kmoney),
               'USD 999999999999999999.999999999999999999'::kmoney),
           kmoney_sum_accum(NULL::bytea,
               'USD -999999999999999999.999999999999999999'::kmoney)))::text || ' | '
    || kmoney_sum_final(kmoney_sum_combine(
           kmoney_sum_accum(NULL::bytea,
               'USD -999999999999999999.999999999999999999'::kmoney),
           kmoney_sum_accum(kmoney_sum_accum(NULL::bytea,
               'USD 999999999999999999.999999999999999999'::kmoney),
               'USD 999999999999999999.999999999999999999'::kmoney)))::text;

\echo -- the_sum_aggregate_combines_an_empty_partial
-- A worker that scanned no rows contributes a NULL state, and merging it must change nothing.
SELECT 'empty_partial_is_identity=' || (
    kmoney_sum_final(kmoney_sum_combine(kmoney_sum_accum(NULL::bytea, 'USD 1.00'::kmoney), NULL))
    = 'USD 1.00'::kmoney);

\echo -- the_sum_aggregate_rejects_a_forged_transition_state
-- The state type is bytea, so these support functions are callable by hand with arbitrary bytes.
-- That must be an error, not a misread -- the same rule the binary RECEIVE path follows.
SELECT kmoney_sum_final('\xdeadbe'::bytea)::text;

\echo -- the_sum_aggregate_rejects_a_total_that_leaves_the_domain
-- The domain check fires ONCE, at the end, on the true total -- not per partial, which is what
-- made the removed narrow-state aggregate plan-dependent.
CREATE TEMP TABLE edge_ledger (amount kmoney);
INSERT INTO edge_ledger VALUES ('USD 999999999999999999.999999999999999999'),
                               ('USD 0.000000000000000001');
SELECT sum(amount)::text FROM edge_ledger;

\echo -- the_sum_aggregate_refuses_a_mixed_currency_column
CREATE TEMP TABLE mixed_ledger (amount kmoney);
INSERT INTO mixed_ledger VALUES ('USD 1.00'), ('IDR 1.00');
SELECT sum(amount)::text FROM mixed_ledger;

\echo == CASE COMPLETE: 04-sum ==
