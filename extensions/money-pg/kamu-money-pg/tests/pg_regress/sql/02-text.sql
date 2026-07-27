-- 02-text: the canonical text form, its refusals, and the phase-4/phase-5 differential.
--
-- Ports: the_text_form_matches_money_core, kmoney_refuses_what_numeric_silently_rounds,
-- the_domain_top_round_trips, one_unit_past_the_domain_is_refused,
-- an_unknown_currency_is_refused_at_the_boundary, the_native_type_and_the_text_storage_agree.
--
-- The refusal probes below are why the whole suite runs under ON_ERROR_STOP=0: several cases
-- exist to provoke an ERROR whose TEXT is the assertion. VERBOSITY terse keeps that to the one
-- line kmoney owns -- DETAIL, HINT, CONTEXT and the statement-position caret are the server's
-- and differ with version and client.
\pset pager off
\pset footer off
\pset format unaligned
\pset tuples_only on
\pset null '<NULL>'
\set VERBOSITY terse
SET client_min_messages = error;
CREATE EXTENSION IF NOT EXISTS kmoney;

\echo -- the_text_form_matches_money_core
-- Liberal in, canonical out; and the settlement exponent decides the trim, per currency.
SELECT 'USD 10.50'::kmoney::text || ' | '
    || 'USD 10.5'::kmoney::text  || ' | '
    || 'JPY 10.5'::kmoney::text  || ' | '
    || 'KWD 10.5'::kmoney::text  || ' | '
    || 'USD -0.000000000000000001'::kmoney::text;

\echo -- the_domain_top_round_trips
-- The bound is <=, not <.
SELECT 'IDR 999999999999999999.999999999999999999'::kmoney::text;

\echo -- kmoney_refuses_what_numeric_silently_rounds
-- The other half of E13. An input function runs BEFORE any coercion, where a CHECK or a DOMAIN
-- runs after and is handed the already-altered value.
SELECT 'USD 0.0000000000000000004'::kmoney::text;

\echo -- one_unit_past_the_domain_is_refused
SELECT 'IDR 1000000000000000000'::kmoney::text;

\echo -- an_unknown_currency_is_refused_at_the_boundary
SELECT 'ZWL 1.00'::kmoney::text;

\echo -- the_native_type_and_the_text_storage_agree
-- THE PHASE 4 <-> PHASE 5 DIFFERENTIAL. Phase 4 stores the canonical text in a `text` column on
-- any PostgreSQL; phase 5 stores this 18-byte type. One literal writes BOTH columns, so if
-- kamu_money_core::text and this extension's in/out functions ever diverge, an application
-- reading through the driver and a query reading the native column return different numbers for
-- the same row.
CREATE TEMP TABLE both_forms (phase4 text NOT NULL, phase5 kmoney NOT NULL);
INSERT INTO both_forms VALUES
    ('USD 10.50', 'USD 10.50'),
    ('JPY 10.5', 'JPY 10.5'),
    ('KWD 10.500', 'KWD 10.500'),
    ('IDR 999999999999999999.999999999999999999', 'IDR 999999999999999999.999999999999999999'),
    ('USD -0.000000000000000001', 'USD -0.000000000000000001'),
    ('XAU 10.5', 'XAU 10.5');
SELECT 'render_disagreements=' || count(*) FROM both_forms WHERE phase4 <> phase5::text;
SELECT 'reparse_mismatches=' || count(*) FROM both_forms WHERE phase4::kmoney::text <> phase5::text;

\echo == CASE COMPLETE: 02-text ==
