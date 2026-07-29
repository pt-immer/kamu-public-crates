-- 07-allocate: the operation that conserves exactly, and the two things it refuses.
--
-- Ports: allocation_conserves_the_total_exactly, allocation_puts_the_odd_unit_on_the_first_share,
-- allocation_never_pays_a_zero_weight_recipient, allocation_honours_weights_and_still_conserves,
-- allocation_refuses_weights_that_sum_to_zero, allocation_refuses_a_null_weight.
--
-- WITH ORDINALITY + ORDER BY ord, where the Rust tests rely on unnest's natural order. Same
-- result, but the ordering is asked for rather than assumed -- and this suite has to produce
-- identical bytes on two engines whose executors are not the same code.
\pset pager off
\pset footer off
\pset format unaligned
\pset tuples_only on
\pset null '<NULL>'
\set VERBOSITY terse
SET client_min_messages = error;
CREATE EXTENSION IF NOT EXISTS kmoney;

\echo -- allocation_conserves_the_total_exactly
SELECT kmoney_sum(VARIADIC array_agg(part))::text
  FROM unnest(kmoney_allocate('USD 10.00', ARRAY[1, 1, 1])) AS part;

\echo -- allocation_puts_the_odd_unit_on_the_first_share
-- The shares are at the canonical scale, NOT rounded to the currency's minor unit -- rounding
-- here would move money silently.
SELECT string_agg(part::text, ' | ' ORDER BY ord)
  FROM unnest(kmoney_allocate('USD 10.00', ARRAY[1, 1, 1])) WITH ORDINALITY AS t(part, ord);

\echo -- allocation_never_pays_a_zero_weight_recipient
-- One canonical unit across weights [0, 1, 1] leaves a 1-unit remainder that must reach
-- the first POSITIVE slot, never the zero at index 0: money conserved AND paid to a party with
-- a claim.
SELECT string_agg(part::text, ' | ' ORDER BY ord)
  FROM unnest(kmoney_allocate('USD 0.000000000000000001', ARRAY[0, 1, 1]))
       WITH ORDINALITY AS t(part, ord);

\echo -- allocation_honours_weights_and_still_conserves
SELECT kmoney_sum(VARIADIC array_agg(part))::text
  FROM unnest(kmoney_allocate('IDR 16000.01', ARRAY[7, 2, 1])) AS part;

\echo -- allocation_refuses_weights_that_sum_to_zero
SELECT kmoney_allocate('USD 10.00', ARRAY[0, 0])::text;

\echo -- allocation_refuses_a_null_weight
SELECT kmoney_allocate('USD 10.00', ARRAY[1, NULL])::text;

\echo -- allocation_accepts_exactly_the_documented_limit
-- The weight count is chosen at run time by whoever wrote the query, and a PostgreSQL array is
-- bounded only by the 1GB varlena limit, so the boundary states a limit rather than discovering
-- one when a backend runs out of memory. Both sides of it are exercised: this is the last
-- accepted size, and conservation still holds there.
SELECT kmoney_sum(VARIADIC kmoney_allocate('USD 10.00',
           (SELECT array_agg(1) FROM generate_series(1, 65536))))::text;

\echo -- allocation_refuses_more_parts_than_the_documented_limit
-- One past the limit, not a wild number: a probe with 268 million weights would prove the same
-- thing while spending a gigabyte to do it, and would not notice an off-by-one in the comparison.
SELECT kmoney_allocate('USD 10.00',
           (SELECT array_agg(1) FROM generate_series(1, 65537)))::text;

\echo == CASE COMPLETE: 07-allocate ==
