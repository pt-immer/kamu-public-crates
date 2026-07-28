# Containerised pgrx build + test for one PostgreSQL major.
#
# Nothing is installed on the host: the extension is compiled against this container's PG
# headers and loaded into this container's server. That also turns the PG15..latest matrix into
# a loop over build args rather than a sequence of mutually-exclusive host installs.
#
#   docker build -f kamu-money-pg/Dockerfile --build-arg PG_MAJOR=18 -t kamu-money-pg:pg18 .
#   docker run --rm kamu-money-pg:pg18
#
# The whole supported range: kamu-money-pg/test-matrix.sh
#
# LAYER ORDER IS LOAD-BEARING. `cargo install cargo-pgrx` takes minutes and does NOT depend on
# the PostgreSQL major, so it is installed BEFORE `ARG PG_MAJOR` is declared. Everything above
# that line is one cached layer shared by every image in the matrix; only the apt step and
# `pgrx init` below it are rebuilt per version. Declaring the ARG earlier would invalidate the
# cargo layer for each major and multiply the build time by the size of the matrix.

ARG RUST_VERSION=1.97
FROM rust:${RUST_VERSION}-trixie

# Must match the `pgrx` dependency in kamu-money-pg/Cargo.toml exactly. cargo-pgrx refuses to build
# an extension whose pgrx version differs from its own — a good refusal, since the generated
# SQL and the FFI shims are versioned together.
ARG PGRX_VERSION=0.19.1

RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates curl gnupg lsb-release \
    && rm -rf /var/lib/apt/lists/*

# Optional extra trust anchors, BEFORE the first HTTPS fetch below. On a network that terminates
# and re-signs TLS the container sees a chain the host trusts and it does not, and the PGDG key
# fetch dies with `curl: (60) self-signed certificate in certificate chain` — host fine, container
# broken, which is the tell. Normally this directory holds only a README and the step is a no-op
# (update-ca-certificates ignores non-.crt files). See kamu-money-pg/ca-certs/README.md.
#
# THIS COPY CANNOT BE MOVED BELOW `cargo install cargo-pgrx`, and the temptation to move it is
# real: it sits above the most expensive layer in the file, so changing this path invalidates
# the pgrx install for every major. That cost was paid once when the crate directory was
# renamed, and it looked like a layer-ordering mistake. It is not. Both the PGDG key `curl`
# below AND `cargo install`'s fetch from crates.io are HTTPS, so the trust anchors have to be
# in place before either runs. Reordering trades a rare rebuild for a build that cannot
# complete at all on an inspecting network.
COPY kamu-money-pg/ca-certs/ /usr/local/share/ca-certificates/
RUN update-ca-certificates

RUN apt-get update && apt-get install -y --no-install-recommends \
        build-essential libclang-dev clang pkg-config \
        libreadline-dev zlib1g-dev flex bison \
        libxml2-dev libxslt1-dev libssl-dev libxml2-utils xsltproc \
    && install -d /usr/share/postgresql-common/pgdg \
    && curl -fsSL https://www.postgresql.org/media/keys/ACCC4CF8.asc \
         -o /usr/share/postgresql-common/pgdg/apt.postgresql.org.asc \
    && echo "deb [signed-by=/usr/share/postgresql-common/pgdg/apt.postgresql.org.asc] \
         https://apt.postgresql.org/pub/repos/apt $(lsb_release -cs)-pgdg main" \
         > /etc/apt/sources.list.d/pgdg.list \
    && rm -rf /var/lib/apt/lists/*

# PostgreSQL refuses to run as root and `cargo pgrx test` starts a server, so both the build
# and the test run unprivileged.
RUN useradd -m -s /bin/bash pgrx
USER pgrx
ENV PATH="/home/pgrx/.cargo/bin:${PATH}" CARGO_HOME=/home/pgrx/.cargo

# --- shared across the entire matrix; keep this above ARG PG_MAJOR -----------------------
RUN cargo install cargo-pgrx --version "=${PGRX_VERSION}" --locked
# ----------------------------------------------------------------------------------------

ARG PG_MAJOR=18
USER root
RUN apt-get update && apt-get install -y --no-install-recommends \
        "postgresql-${PG_MAJOR}" "postgresql-server-dev-${PG_MAJOR}" \
    && rm -rf /var/lib/apt/lists/* \
    # `cargo pgrx install` (which `cargo pgrx test` runs first) writes the .control and .sql
    # into pg_config --sharedir/extension and the .so into pg_config --pkglibdir. A
    # distro-packaged PostgreSQL owns both as root, so the unprivileged build fails with
    # `Permission denied (os error 13)` before a single test runs -- and pgrx reports it as
    # five test failures, four of them a poisoned-mutex cascade, which hides the real cause.
    #
    # Handing those two directories to the build user is safe *here* and only here: the
    # container is disposable, single-purpose, and has no other tenant. Do NOT do this to a
    # host PostgreSQL. The alternative -- `cargo pgrx init` with no pg_config, which compiles
    # its own PostgreSQL into a user-owned ~/.pgrx -- avoids the chown but tests against a
    # from-source build rather than the PGDG binaries that actually run in production.
    && PGC="/usr/lib/postgresql/${PG_MAJOR}/bin/pg_config" \
    && chown -R pgrx:pgrx "$("$PGC" --sharedir)/extension" "$("$PGC" --pkglibdir)"
USER pgrx

# `USER pgrx` above sets the process UID; it does NOT export a `USER` environment variable --
# that is a login-shell convention, and a container has no login shell. pgrx-tests reads
# `$USER` in `get_pg_user()` to decide which PostgreSQL role to create and connect as, and
# panics outright without it:
#     USER environment var is unset or invalid UTF-8   (pgrx-tests framework.rs, get_pg_user)
# Like the permission failure below, it surfaces as five failed tests of which four are only
# the poisoned-mutex cascade. Read the first one.
ENV USER=pgrx

# Point pgrx at the PG this image already has rather than letting it compile another one.
RUN cargo pgrx init "--pg${PG_MAJOR}" "/usr/lib/postgresql/${PG_MAJOR}/bin/pg_config"

WORKDIR /work
COPY --chown=pgrx . .

# `.cargo/config.toml` pins PGRX_HOME to `<repo>/.pgrx` with `force = true`, so that rule holds
# on a developer's machine AND here -- the point of `force` is that a stray `~/.pgrx` must not
# decide what gets built. Inside the image the real pgrx home was created above, before the
# source was copied, because it sits under the expensive `cargo install cargo-pgrx` layer that
# every PostgreSQL major shares.
#
# A symlink satisfies both facts. Without it the forced path resolves to a `/work/.pgrx` that
# does not exist and pgrx-pg-sys's build script fails with `$PGRX_HOME does not exist`.
# `.pgrx/` is in `.dockerignore`, so nothing was copied here to collide with it.
RUN ln -s /home/pgrx/.pgrx /work/.pgrx

ENV PG_MAJOR=${PG_MAJOR}
CMD cargo pgrx test --no-default-features --features "pg${PG_MAJOR}" "pg${PG_MAJOR}"
