-- 10-hash: the persisted hash pinned to exact numbers, and the operator classes that must NOT
-- exist.
--
-- Ports: the_persisted_hash_values_are_pinned_not_merely_consistent,
-- neither_type_has_an_operator_class.
--
-- THIS IS THE SHARPEST ABI SIGNAL IN THE SUITE. An in-process test that hashed and re-read in
-- the same binary could never fail, which is precisely not the case that matters: what breaks a
-- persisted hash is a REBUILD -- under a different toolchain, or against a different fork's
-- headers -- producing different numbers than the ones already on disk. If the 18-byte payload
-- is read at a wrong offset on YugabyteDB, these four i32 diverge, and silently-wrong money
-- becomes visible.
\pset pager off
\pset footer off
\pset format unaligned
\pset tuples_only on
\pset null '<NULL>'
\set VERBOSITY terse
SET client_min_messages = error;
CREATE EXTENSION IF NOT EXISTS kmoney;

\echo -- the_persisted_hash_values_are_pinned_not_merely_consistent
-- From kamu_money_core::stable_hash, whose golden vectors were cross-checked against an
-- independent implementation; these are the same values after the fold to int4. A change here
-- needs a STABLE_HASH_VERSION bump and a re-hash of any store that persisted the old value --
-- a shard key, a hash partition, a durable cache key.
SELECT 'h_usd_0=' || kmoney_hash('USD 0.00'::kmoney)
    || ' h_usd_1=' || kmoney_hash('USD 1.00'::kmoney)
    || ' h_idr_1=' || kmoney_hash('IDR 1.00'::kmoney)
    || ' h_usd_neg1=' || kmoney_hash('USD -1.00'::kmoney);
-- One payload, two entry points. They share a codec, so two implementations would otherwise be
-- free to drift while each looked right on its own.
SELECT 'native_equals_mixed=' || (kmoney_hash('USD 1.00'::kmoney)
                               = kmoney_mixed_hash('USD 1.00'::kmoney_mixed));

\echo -- neither_type_has_an_operator_class
-- Guarded at the CATALOG rather than by matching version-specific planner error text. With no
-- opclass there is no ORDER BY amount, no value index, no GROUP BY / DISTINCT / UNIQUE on
-- amount -- and, not incidentally, no index access method for YugabyteDB to have replaced.
SELECT 'kmoney_opclasses=' || (SELECT count(*) FROM pg_opclass WHERE opcintype = 'kmoney'::regtype)
    || ' mixed_opclasses=' || (SELECT count(*) FROM pg_opclass WHERE opcintype = 'kmoney_mixed'::regtype);

\echo == CASE COMPLETE: 10-hash ==
