-- 01-layout: the physical shape of `kmoney`, and the space argument stated honestly.
--
-- Ports: kmoney_is_eighteen_bytes_with_no_header,
-- the_catalog_says_fixed_length_plain_and_byte_aligned,
-- the_size_tradeoff_against_numeric_is_measured_not_assumed,
-- the_size_does_not_depend_on_the_value, numeric_silently_rounds_four_e_minus_nineteen_to_zero.
--
-- OUTPUT IS UNALIGNED AND TUPLES-ONLY throughout this suite, on purpose: psql's aligned table
-- rendering pads every column to its widest cell, so a golden file would encode column WIDTHS
-- as well as values and a one-character change in an unrelated row would move them. Every line
-- below is `label=value`, which is exactly as strict about the value and says nothing about
-- the layout.
\pset pager off
\pset footer off
\pset format unaligned
\pset tuples_only on
\pset null '<NULL>'
\set VERBOSITY terse
-- YugabyteDB emits a WARNING about ROWS_PER_TRANSACTION when COPY targets a temp table, which
-- stock PG15 has no notion of. Neither is a kmoney behaviour. ERRORs still print.
SET client_min_messages = error;
CREATE EXTENSION IF NOT EXISTS kmoney;

\echo -- kmoney_is_eighteen_bytes_with_no_header
CREATE TEMP TABLE sized (v kmoney);
INSERT INTO sized VALUES ('USD 10.50');
-- Equal numbers are the tell that nothing is being wrapped: a varlena would read 22 in memory
-- and 19 on disk.
SELECT 'stored=' || pg_column_size(v)
    || ' in_memory=' || pg_column_size('USD 10.50'::kmoney) FROM sized;

\echo -- the_catalog_says_fixed_length_plain_and_byte_aligned
SELECT format('%s=%s/%s/%s/%s', typname, typlen, typbyval, typalign, typstorage)
  FROM pg_type WHERE typname IN ('kmoney', 'kmoney_mixed') ORDER BY typname;

\echo -- the_size_tradeoff_against_numeric_is_measured_not_assumed
CREATE TEMP TABLE compared (r kmoney, n numeric(36,18));
INSERT INTO compared VALUES
    ('USD 10.50', 10.50),
    ('IDR 999999999999999999.999999999999999999', 999999999999999999.999999999999999999);
-- The RELATIONS are asserted, not numeric's exact widths. `numeric` is variable-width and its
-- encoding is PostgreSQL's business; pinning 7 and 23 here would turn any change in someone
-- else's type into a kmoney divergence. The contract is that kmoney stays fixed at 18 and
-- numeric beats it on a typical amount and loses at the top -- which is what this checks.
SELECT 'kmoney_fixed_at_18=' || (count(DISTINCT pg_column_size(r)) = 1 AND max(pg_column_size(r)) = 18)
    || ' numeric_wins_typical=' || (min(pg_column_size(n)) < 18)
    || ' numeric_loses_at_top=' || (max(pg_column_size(n)) > 18)
  FROM compared;

\echo -- the_size_does_not_depend_on_the_value
CREATE TEMP TABLE varied (v kmoney);
INSERT INTO varied VALUES
    ('USD 0.00'), ('USD 10.50'), ('IDR 999999999999999999.999999999999999999'), ('JPY -1');
SELECT 'distinct_sizes=' || count(DISTINCT pg_column_size(v)) FROM varied;

\echo -- numeric_silently_rounds_four_e_minus_nineteen_to_zero
-- PostgreSQL numeric rounds this silently; 02-text asserts that kmoney refuses it.
SELECT 'numeric_rounds_4e_minus_19_to_zero=' || ('0.0000000000000000004'::numeric(36,18) = 0);

\echo == CASE COMPLETE: 01-layout ==
