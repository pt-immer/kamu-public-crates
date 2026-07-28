-- kamu-money-pg kmoney: byte-exact ABI battery. Run IDENTICALLY on YugabyteDB 2025.2.x
-- (patched pgrx 0.19.1) and on stock PostgreSQL 15, then diff the two outputs.
-- Both are PG15, so any divergence is a pure YB-fork-ABI effect on kamu-money-pg's
-- custom-type code path -- the part the exp-pgrx-yb uuid probe could not cover.
--
--   psql -X -v ON_ERROR_STOP=0 -f abi_battery.sql > out.txt 2>&1
--
-- ON_ERROR_STOP=0: several probes below EXPECT an error, and the error TEXT is
-- part of what must match A/B. Deterministic output only (no timestamps/oids).
--
-- SCOPE: kmoney is an amount SCALAR for OLTP wallet/ledger schemas -- native storage +
-- arithmetic + equality. It is a column type, not a store: this extension implements no
-- account, transaction or balance. Amount columns are assumed not to be ordered/ranged/
-- indexed by value (that is OLAP, and a custom type's default-opclass ordering was the sole
-- YugabyteDB edge), so there is no btree or hash opclass, no ORDER BY amount, and no value
-- index anywhere in this battery.

\pset pager off
\pset footer off
\set ECHO none
CREATE EXTENSION IF NOT EXISTS kmoney;

-- Normalize infrastructure chatter so the A/B diff compares kmoney's behavior, not the
-- server's: YugabyteDB emits a WARNING about ROWS_PER_TRANSACTION batching when COPY targets
-- a temp table (§2b), which stock PG15 has no notion of. Neither is an kmoney effect. ERRORs
-- (which the refusal probes assert) are level ERROR and still print.
SET client_min_messages = error;

\echo == 1. send/recv + text round-trip (custom FromDatum/IntoDatum over 18 bytes) ==
SELECT 'USD 10.50'::kmoney::text AS usd,
       'IDR 16000.01'::kmoney::text AS idr,
       'JPY 10.5'::kmoney::text AS jpy,
       'KWD 10.500'::kmoney::text AS kwd,
       ('USD -0.000000000000000001'::kmoney)::text AS tiny_neg,
       ('USD 999999999999999999.999999999999999999'::kmoney)::text AS domain_top;
SELECT typname, typlen, typbyval, typalign, typstorage
  FROM pg_type WHERE typname IN ('kmoney', 'kmoney_mixed') ORDER BY typname;

\echo == 2. binary send() width + bytea (raw SEND path) ==
SELECT length(kmoney_send('USD 10.50'::kmoney)) AS send_len,
       encode(kmoney_send('USD 1.00'::kmoney), 'hex') AS usd_one_hex;

\echo == 2b. COPY (FORMAT BINARY) round trip: send out, recv back in ==
-- The only in-DB path that actually invokes kmoney_recv -- its arg is `internal`,
-- unreachable from a SQL literal. R2-F5: the earlier test used INSERT .. SELECT and
-- touched neither send nor recv. Server-side file; single ysqlsh session, fixed name.
CREATE TEMP TABLE wire_src (amount kmoney);
INSERT INTO wire_src VALUES ('IDR -16000.50'),
                            ('USD 0.000000000000000001'),
                            ('USD 999999999999999999.999999999999999999');
COPY wire_src TO '/tmp/kmoney_abi_wire.bin' (FORMAT BINARY);
CREATE TEMP TABLE wire_dst (LIKE wire_src);
COPY wire_dst FROM '/tmp/kmoney_abi_wire.bin' (FORMAT BINARY);
SELECT (SELECT count(*) FROM wire_dst) AS rows_recv,
       (SELECT array_agg(amount::text ORDER BY amount::text) FROM wire_src)
         = (SELECT array_agg(amount::text ORDER BY amount::text) FROM wire_dst)
         AS roundtrip_exact;

\echo == 3. arithmetic: + - and cross-currency refusal ==
SELECT ('USD 10.50'::kmoney + 'USD 0.25'::kmoney)::text AS sum,
       ('USD 10.50'::kmoney - 'USD 0.25'::kmoney)::text AS diff;
\echo -- cross-currency + must ERROR (message must match A/B):
SELECT ('USD 1.00'::kmoney + 'IDR 1.00'::kmoney)::text AS must_error;

\echo == 4. kmoney_sum (VariadicArray + UnboxDatum) incl the domain-edge transient ==
SELECT kmoney_sum('USD 10.50','USD 0.25','USD 0.25')::text AS s;
SELECT kmoney_sum('USD 999999999999999999.999999999999999999',
                  'USD 999999999999999999.999999999999999999',
                  'USD -999999999999999999.999999999999999999')::text AS transient;
SELECT kmoney_sum(VARIADIC ARRAY[]::kmoney[])::text AS empty_is_null;
\echo -- mixed-currency variadic must ERROR:
SELECT kmoney_sum('USD 1.00','IDR 1.00')::text AS must_error;

\echo == 5. kmoney_allocate incl the R2-F1 zero-weight guard ==
SELECT string_agg(part::text, ' | ') AS even
  FROM unnest(kmoney_allocate('USD 10.00', ARRAY[1,1,1])) part;
SELECT string_agg(part::text, ' | ') AS zero_weight
  FROM unnest(kmoney_allocate('USD 0.000000000000000001', ARRAY[0,1,1])) part;
SELECT kmoney_sum(VARIADIC array_agg(part))::text AS conserves
  FROM unnest(kmoney_allocate('IDR 16000.01', ARRAY[7,2,1])) part;

\echo == 6. comparison operators as PREDICATES (same-currency orders; cross-currency refuses) ==
SELECT ('USD 1.00'::kmoney <  'USD 2.00'::kmoney) AS lt,
       ('USD 2.00'::kmoney >= 'USD 2.00'::kmoney) AS ge,
       ('USD 1.00'::kmoney =  'IDR 1.00'::kmoney) AS cross_eq_false;
-- `=` is TOTAL, so it filters a mixed column without raising; ordering filters same-currency.
CREATE TEMP TABLE pred (amount kmoney);
INSERT INTO pred VALUES ('USD 1.00'),('USD 2.00'),('IDR 1.00'),('USD 1.00');
SELECT count(*) AS usd_ones FROM pred WHERE amount = 'USD 1.00'::kmoney;
\echo -- cross-currency ORDERING must ERROR (message must match A/B):
SELECT ('IDR 1.00'::kmoney > 'USD 1.00'::kmoney) AS must_error;

\echo == 7. PINNED hash values (the sharpest custom-type ABI signal) ==
-- These i32 come from kamu_money_core::stable_hash (F3 golden vectors). If the 18-byte
-- payload is read at a wrong offset on YB, these diverge -- silently-wrong money
-- made visible. kmoney_hash is a plain function now (no hash opclass/index).
SELECT kmoney_hash('USD 0.00'::kmoney)  AS h_usd_0,
       kmoney_hash('USD 1.00'::kmoney)  AS h_usd_1,
       kmoney_hash('IDR 1.00'::kmoney)  AS h_idr_1,
       kmoney_hash('USD -1.00'::kmoney) AS h_usd_neg1;
SELECT kmoney_hash('USD 1.00'::kmoney) = kmoney_mixed_hash('USD 1.00'::kmoney_mixed) AS same_payload_same_hash;

\echo == 8. the sum aggregate on kmoney, and its absence on kmoney_mixed ==
SELECT ('USD 1.00'::kmoney_mixed = 'IDR 1.00'::kmoney_mixed) AS mixed_cross_eq_false;
\echo -- sum(kmoney) EXISTS, with a wide transition state (R2-F4b):
-- Two rows whose PARTIAL sum leaves the domain while the total does not. The aggregate R2-F4
-- removed had a `kmoney` transition state and failed here; this one accumulates in I256 and
-- checks the domain once. On the ABI surface this also exercises a bytea transition state
-- crossing the fmgr boundary on every row, which nothing else in this battery does.
SELECT sum(a)::text AS agg_across_a_domain_edge
  FROM (VALUES ('USD 999999999999999999.999999999999999999'::kmoney),
               ('USD 999999999999999999.999999999999999999'::kmoney),
               ('USD -999999999999999999.999999999999999999'::kmoney)) t(a);
\echo -- but sum(kmoney_mixed) must still NOT exist -- that is the whole point of the mixed type:
SELECT sum(a) FROM (VALUES ('USD 1.00'::kmoney_mixed)) t(a);

\echo == 9. domain + precision refusals (parse path) ==
-- The ISO prefix is REQUIRED here. Without it this dies in the literal parser
-- ("invalid money literal") and the domain branch is never reached -- the probe would then
-- claim domain coverage it does not actually have.
SELECT 'USD 1000000000000000000.00'::kmoney;             -- one past the domain: ERROR
SELECT 'USD 0.0000000000000000005'::kmoney;              -- 19dp: ERROR, never rounded

\echo == BATTERY COMPLETE ==
