# Vendored ISO 4217 register

`src/iso.rs`'s currency table is **generated from `vendor/list-one.xml`**, not
authored. This document exists so that the claim "these are the ISO 4217
currencies" is checkable against something in the repository rather than against
a URL that can move, a page that can change, or an author's memory.

## Credit

The register is published by **SIX Group AG (SIX Financial Information)**, the
**maintenance agency for ISO 4217** on behalf of ISO. The list is theirs; this
project only transforms it.

| Field | Value |
|---|---|
| Source | `https://www.six-group.com/dam/download/financial-information/data-center/iso-currrency/lists/list-one.xml` |
| Maintenance agency | SIX Group AG, on behalf of ISO |
| Standard | ISO 4217 — Codes for the representation of currencies |
| Published (file's own `Pblshd`) | 2026-01-01 |
| Captured | 2026-07-22 |
| SHA-256 | `838dfb991648cf36df939edd5fe3811737962b75a32252847d239cedd1e291c9` |
| `<CcyNtry>` rows | 280 |
| Of those, carrying a `<Ccy>` | 277 |
| Distinct currency codes | 178 |

The `iso-currrency` misspelling in the path is upstream's, not a typo here.

Every number above is checked against the file rather than typed beside it. The
publication date and the three counts are verified while `build.rs` parses, so a
mismatch is a **build failure** in every crate downstream. The SHA-256 is
verified by a test, because hashing needs a dependency the build does not
otherwise want.

## Redistribution

The file is vendored so the register travels with the checkout. The position,
stated plainly:

- **ISO explicitly permits free-of-charge use of the ISO 3166, 4217 and 639
  codes.** That is the standards body's own published stance, not an inference.
- **SIX publishes this list free of charge** and states no terms alongside it —
  no copyright notice, no licence, no redistribution grant, no disclaimer.
- **The facts are data.** That `USD` is 840 with two minor digits is not
  copyrightable in most jurisdictions; what could attract protection is the
  compilation, not the numbers in it.

Assessed risk: **low**. This is an assessment, not legal advice. Vendoring
ISO 4217 data is ordinary practice across open-source projects, ISO blesses use
of the codes, and a 178-row currency table inside a money library is not the
wholesale appropriation that database-compilation rights exist to address.

This project asserts no licence over `list-one.xml` and claims no ownership of
the register.

### What the package's licence does and does not cover

`Cargo.toml` declares `MIT OR Apache-2.0` and the package ships both texts.
Stated precisely, so the two facts on this page are not read as one:

- **The licence covers this crate's own work** — the build script, the parser,
  the validation, the emitted code, and this document.
- **It does not, and cannot, cover `vendor/list-one.xml`.** That file is not
  ours to license. It is redistributed as third-party data under the position
  set out above, with credit to SIX Group AG as ISO's maintenance agency.

A single `license` field cannot express that split — cargo has one slot and this
package has two provenances. This section is where the distinction lives.

## Regenerating

There is no generated file to keep in step: `build.rs` reads this XML during
compilation, so the register and the table are the same object.

To move to a newer edition:

1. Download the current `list-one.xml` over this one. **The build now fails**,
   naming the first fact that no longer matches.
2. Update the `edition` manifest in `build/iso4217.rs`: published date, SHA-256,
   and the three counts.
3. Update the provenance table above to match. A test compares the recorded
   digest against the file, so a half-done step 2 cannot pass as complete.
4. Update the pinned count in `src/iso.rs`'s
   `the_register_matches_the_edition_it_was_generated_from`.
5. If `src/iso.rs`'s `the_alpha3_to_numeric_mapping_is_frozen` fails, a
   currency's IDENTITY moved — apply the append-only policy below before
   re-blessing its digest.

Each failure is the intended prompt for the next step, not an obstacle to route
around. The ordering matters: step 1 breaks the build rather than the tests, so
a replaced file cannot reach a green suite by way of a forgotten edit.

## Identity facts are append-only

Persisted data outlives register editions. Stored `kmoney_mixed` payloads
resolve their 2-byte numeric against the compiled register at every read,
`kamu-money-pg` derives one SQL type per code, and
`stable_hash(code.numeric(), units)` values are persisted by downstream
systems. So the `(alpha3, numeric)` mapping is a persistence contract, not
reference data:

- **Codes are never removed.** When ISO withdraws a currency, the register
  keeps it. Removing a code deletes its derived SQL type and its recv/out
  symbols while production catalogs still reference them, and makes stored
  mixed rows of that currency unreadable — including by `pg_dump`, which is
  the only migration path.
- **Numeric codes never change.** A changed numeric silently re-labels stored
  mixed money as another currency and moves every persisted hash without a
  `STABLE_HASH_VERSION` bump. If ISO ever reuses or renumbers a code, that is
  a `STABLE_HASH_VERSION` decision, not a register refresh.
- **Additions are ordinary.** A new code adds a new type and disturbs nothing
  stored. Exponent and name changes are also ordinary for STORAGE — canonical
  units are scale-18 regardless of exponent — but an exponent change shifts
  rendered text (trailing zeros), which moves goldens and any `::text`
  expression index, and deserves its own release note.

`src/iso.rs`'s `the_alpha3_to_numeric_mapping_is_frozen` pins the digest of the
full mapping so none of this can happen as a side effect of step 1.

## What the build script checks, as it reads

Every one of these is a **build failure**, not a test — so a bad register cannot
be skipped past with `--skip` or forgotten by whoever replaces the file:

- **Internal agreement.** A currency used in several countries appears once per
  country. Every such row must agree on the numeric code and the minor units, or
  the table would depend on which row happened to be parsed first.
- **Numeric uniqueness.** No two currencies may share a numeric code —
  `from_numeric` would be lossy, and the binary serde form encodes precisely
  that number.
- **Shape.** Three ASCII uppercase letters, a numeric code in `0..=999`, a
  minor-unit count no greater than 4 (ISO's widest, `CLF`), and a non-empty name.

## Why the table is not hand-written

Minor-unit exponents look memorable and are not: of the 178 codes, 17 take **0**
fraction digits, 139 take **2**, 7 take **3**, 2 take **4**, and 13 have **none
at all**. A table typed from recollection would review as correct and settle
amounts wrongly — the exact failure this crate exists to prevent, introduced
through its own reference data.
