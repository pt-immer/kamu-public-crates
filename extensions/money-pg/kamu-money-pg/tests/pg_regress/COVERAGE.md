# Case-suite coverage of the `#[pg_test]` contract

This file is the manifest mapping every `#[pg_test]` in `kamu-money-pg` to the SQL case that
restates it. It is **machine-checked**: `hygiene/tests/repo_hygiene.rs`
(`the_case_suite_accounts_for_every_pg_test`) parses **every `.rs` file under
`kamu-money-pg/src/`** for `#[pg_test]` and `#[pg_test(...)]` attributes, reads the table below,
and fails if a test is missing from it, if a named case file does not exist, or if the table names
a test that no longer does.

It scans the crate rather than `lib.rs` because the 2026-07-27 module split moved the suite beside
the code it tests — `ops.rs`, `wire.rs`, `typmod.rs` and the rest. A manifest check keyed to one
file stops checking the moment the tests move, which is exactly when one can go missing.

That check runs in `just test` — no Docker, no database. It is the mechanism the readiness plan
asks for: *a skipped test that is silently counted as a pass is worse than an absent one.*

## Why the port exists

`cargo pgrx test` manages its own PostgreSQL and cannot be aimed at YugabyteDB. Until this suite,
everything known about `kmoney` on YugabyteDB came from one ~112-line script
(`kamu-money-pg/yb/abi_battery.sql`), while the 65 tests that actually encode this type's contract
had only ever run on PGDG PostgreSQL. Restating them as `sql/` + `expected/` pairs makes them run
against any live server: a single YB node, any node of a YB cluster, or the stock-PG15 reference.

## What is deliberately different from the Rust originals

- **Grouping.** In the backend a raised `ERROR` longjmps out of SPI and aborts the whole test
  transaction, so every refusal needed its own `#[pg_test]`. Under psql each statement is its own
  implicit transaction, so a refusal and the successes around it live in one file. Same
  assertions, fewer sessions.
- **`numeric` widths.** `the_size_tradeoff_against_numeric_is_measured_not_assumed` asserts the
  *relations* (kmoney fixed at 18; numeric smaller for a typical amount, larger at the domain
  top), exactly as the Rust test does. The measured values 7 and 23 are not pinned, because
  `numeric`'s encoding is PostgreSQL's business and pinning it here would turn a change in someone
  else's type into a `kmoney` divergence.
- **Crafted `recv` payloads.** The Rust tests build the malformed BINARY COPY files inside the
  backend with `std::fs`. `sql/09-wire.setup.sh` writes them on the server from pinned constants
  instead. That is stronger provenance, not weaker: the payload is fixed in the repository rather
  than produced by the code under test — and `09-wire.sql` asserts that a live `kmoney_send` still
  emits exactly those bytes, so the two cannot drift apart.

**Nothing is unported.** If a test ever cannot be expressed here, its row must read
`NOT-PORTABLE: <reason>` and the guard accepts it — but it has to say so out loud.

## The map

| # | `#[pg_test]` | Case | Assertion label in the golden |
|---|---|---|---|
| 1 | `kmoney_is_eighteen_bytes_with_no_header` | `01-layout` | `stored=18 in_memory=18` |
| 2 | `the_catalog_says_fixed_length_plain_and_byte_aligned` | `01-layout` | `18/f/c/p` |
| 3 | `the_size_tradeoff_against_numeric_is_measured_not_assumed` | `01-layout` | `kmoney_fixed_at_18=` |
| 4 | `the_size_does_not_depend_on_the_value` | `01-layout` | `distinct_sizes=1` |
| 5 | `the_text_form_matches_money_core` | `02-text` | the five-form line |
| 6 | `numeric_silently_rounds_four_e_minus_nineteen_to_zero` | `01-layout` | `numeric_rounds_4e_minus_19_to_zero=true` |
| 7 | `kmoney_refuses_what_numeric_silently_rounds` | `02-text` | ERROR, 19 fractional digits |
| 8 | `the_domain_top_round_trips` | `02-text` | `IDR 999999999999999999.999999999999999999` |
| 9 | `one_unit_past_the_domain_is_refused` | `02-text` | ERROR, money domain overflow |
| 10 | `an_unknown_currency_is_refused_at_the_boundary` | `02-text` | ERROR, not a money literal |
| 11 | `addition_within_one_currency_is_exact` | `03-arith` | `USD 11.00 \| USD 10.00` |
| 12 | `addition_is_exact_at_one_unit_of_the_eighteenth_decimal` | `03-arith` | the domain top |
| 13 | `addition_across_currencies_is_refused_at_runtime` | `03-arith` | ERROR, USD + IDR |
| 14 | `addition_past_the_domain_top_is_refused` | `03-arith` | ERROR, result outside the domain |
| 15 | `kmoney_sum_adds_an_explicit_list_within_one_currency` | `04-sum` | `USD 11.00` |
| 16 | `kmoney_sum_is_order_independent_across_a_domain_edge_transient` | `04-sum` | three orders, one total |
| 17 | `kmoney_sum_of_nothing_is_null` | `04-sum` | `empty_sum_is_null=true` |
| 18 | `kmoney_sum_rejects_a_mixed_currency_argument` | `04-sum` | ERROR, cannot sum USD and IDR |
| 19 | `kmoney_sum_rejects_a_total_that_leaves_the_domain` | `04-sum` | ERROR, kmoney_sum overflow |
| 20 | `the_sum_aggregate_totals_a_column` | `04-sum` | `USD 11.00` |
| 21 | `the_sum_aggregate_of_nothing_is_null` | `04-sum` | `empty_group_is_null=true` |
| 22 | `the_sum_aggregate_agrees_with_the_variadic_form` | `04-sum` | `aggregate_equals_variadic=true` |
| 23 | `the_sum_aggregate_is_plan_independent_across_a_domain_edge_transient` | `04-sum` | two worker orders, one total |
| 24 | `the_sum_aggregate_combines_an_empty_partial` | `04-sum` | `empty_partial_is_identity=true` |
| 25 | `the_sum_aggregate_rejects_a_forged_transition_state` | `04-sum` | ERROR, state must be 34 bytes |
| 26 | `the_sum_aggregate_rejects_a_total_that_leaves_the_domain` | `04-sum` | ERROR, sum(kmoney) overflow |
| 27 | `the_sum_aggregate_refuses_a_mixed_currency_column` | `04-sum` | ERROR, cannot sum USD and IDR |
| 28 | `sum_on_a_mixed_column_fails_at_plan_time` | `05-mixed` | ERROR, `sum(kmoney_mixed)` |
| 29 | `a_mixed_column_equality_is_currency_aware_and_never_raises` | `05-mixed` | `cross_currency_eq=false usd_ones=2` |
| 30 | `a_mixed_column_cannot_be_ordered` | `05-mixed` | ERROR, operator `<` |
| 31 | `addition_on_the_mixed_type_does_not_exist_either` | `05-mixed` | ERROR, operator `+` |
| 32 | `a_mixed_column_stores_several_currencies_side_by_side` | `05-mixed` | the three-currency render |
| 33 | `the_conversion_out_of_mixed_proves_the_currency` | `05-mixed` | `USD 2.50` |
| 34 | `the_conversion_out_of_mixed_refuses_the_wrong_currency` | `05-mixed` | ERROR, expected USD found IDR |
| 35 | `the_native_type_and_the_text_storage_agree` | `02-text` | `render_disagreements=0` / `reparse_mismatches=0` |
| 36 | `there_is_no_cast_to_numeric` | `11-compare` | ERROR, cannot cast to numeric |
| 37 | `division_returns_the_residue_beside_the_quotient` | `06-div` | quotient and residue |
| 38 | `the_division_identity_holds_for_every_rounding_mode` | `06-div` | seven `<mode>=true` lines |
| 39 | `division_refuses_an_unknown_rounding_mode` | `06-div` | ERROR, "bankers" |
| 40 | `allocation_conserves_the_total_exactly` | `07-allocate` | `USD 10.00` |
| 41 | `allocation_puts_the_odd_unit_on_the_first_share` | `07-allocate` | the three shares |
| 42 | `allocation_never_pays_a_zero_weight_recipient` | `07-allocate` | `USD 0.00 \| ... \| USD 0.00` |
| 43 | `allocation_honours_weights_and_still_conserves` | `07-allocate` | `IDR 16000.01` |
| 44 | `allocation_refuses_weights_that_sum_to_zero` | `07-allocate` | ERROR, weights sum to zero |
| 45 | `allocation_refuses_a_null_weight` | `07-allocate` | ERROR, NULL weight |
| 46 | `allocation_accepts_exactly_the_documented_limit` | `07-allocate` | `USD 10.00` at 65536 parts |
| 47 | `allocation_refuses_more_parts_than_the_documented_limit` | `07-allocate` | ERROR, 65537 exceeds 65536 |
| 48 | `a_typmod_column_round_trips_its_currency` | `08-typmod` | `kmoney('IDR')` |
| 49 | `a_typmod_column_refuses_the_wrong_currency` | `08-typmod` | ERROR, declared IDR but value is USD |
| 50 | `the_binary_wire_round_trips_and_is_not_more_trusted_than_text` | `09-wire` | `has_send` / `send_width` / `usd_one_hex` / `roundtrip_exact` |
| 51 | `equality_is_currency_aware_and_never_raises` | `11-compare` | `cross_eq=false ... gt_within_one_currency=true` |
| 52 | `ordering_refuses_cross_currency` | `11-compare` | ERROR, IDR > USD |
| 53 | `neither_type_has_an_operator_class` | `10-hash` | `kmoney_opclasses=0 mixed_opclasses=0` |
| 54 | `recv_refuses_an_out_of_domain_binary_payload` | `09-wire` | ERROR, received USD outside the domain |
| 55 | `recv_refuses_a_truncated_binary_payload` | `09-wire` | ERROR, insufficient data left |
| 56 | `recv_refuses_a_binary_payload_with_trailing_bytes` | `09-wire` | ERROR, invalid message format |
| 57 | `recv_refuses_a_binary_payload_whose_currency_is_unknown` | `09-wire` | ERROR, ISO code 0 |
| 58 | `the_mixed_recv_entry_point_validates_too` | `09-wire` | ERROR, kmoney_mixed outside the domain |
| 59 | `the_persisted_hash_values_are_pinned_not_merely_consistent` | `10-hash` | the four pinned `i32` |
| 60 | `an_unpinned_column_still_accepts_every_currency` | `08-typmod` | `rows=2` |
| 61 | `a_typmod_of_an_unknown_currency_is_refused` | `08-typmod` | ERROR, "ZWL" |
| 62 | `two_type_modifiers_are_refused` | `08-typmod` | ERROR, got 2 |
| 63 | `typmod_does_not_reach_operators_so_the_value_check_still_fires` | `08-typmod` | ERROR, IDR + USD |
| 64 | `the_planner_splits_the_sum_aggregate_and_both_plans_agree` | NOT-PORTABLE: it asserts a stock-PostgreSQL PLAN (`Partial Aggregate` + `Finalize Aggregate`), and YugabyteDB's planner need not choose the same shape — which is the whole reason `04-sum` drives the transition and combine functions by hand instead. Running on PG15–18 via `just test-pg` is the point: it catches a `CREATE AGGREGATE` declared so that partial aggregation is never available, which every hand-driven test passes straight through. | — |
| 65 | `the_conversion_out_of_mixed_refuses_corrupt_units` | NOT-PORTABLE: SQL text and binary receive reject out-of-domain units before they can become a `kmoney_mixed`; this defense-in-depth test constructs the otherwise unreachable corrupt Rust value directly. | — |

## Running it

```sh
just test-yb-regress     # a live single-node YugabyteDB, and the stock-PG15 reference
just test-yb-cluster     # every node of a 3-node RF=3 cluster
```

Or by hand, against anything that speaks the protocol and has `kmoney` installed:

```sh
kamu-money-pg/tests/pg_regress/run-suite.sh \
    --client "psql -h 127.0.0.1 -p 5432 -U postgres" --label local
```
