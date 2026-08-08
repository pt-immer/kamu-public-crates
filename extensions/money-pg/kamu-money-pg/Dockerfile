# syntax=docker/dockerfile:1
# Containerised pgrx build + test for one PostgreSQL major.
#
# Nothing is installed on the host: the extension is compiled against this container's PG
# headers and loaded into this container's server. That also turns the PG15..latest matrix into
# a loop over build args rather than a sequence of mutually-exclusive host installs.
#
#   just pg test-pg 18
#
# The whole supported range: `just pg test-pg`.
#
# Keep cargo-pgrx installation above `ARG PG_MAJOR` so the matrix shares that layer.

ARG RUST_VERSION=1.96
FROM rust:${RUST_VERSION}-trixie

# Must match the `pgrx` dependency in kamu-money-pg/Cargo.toml exactly. cargo-pgrx refuses to build
# an extension whose pgrx version differs from its own — a good refusal, since the generated
# SQL and the FFI shims are versioned together.
ARG PGRX_VERSION=0.19.1

RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates curl gnupg lsb-release \
    && rm -rf /var/lib/apt/lists/*

# Install optional trust anchors before any HTTPS fetch. The default directory contains only a
# README; update-ca-certificates ignores it. See kamu-money-pg/ca-certs/README.md.
COPY kamu-money-pg/ca-certs/ /usr/local/share/ca-certificates/
RUN update-ca-certificates

RUN apt-get update && apt-get install -y --no-install-recommends \
        build-essential libclang-dev clang jq pkg-config \
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

# Shared across the PostgreSQL matrix.
RUN cargo install cargo-pgrx --version "=${PGRX_VERSION}" --locked

ARG PG_MAJOR=18
USER root
RUN apt-get update && apt-get install -y --no-install-recommends \
        "postgresql-${PG_MAJOR}" "postgresql-server-dev-${PG_MAJOR}" \
    && rm -rf /var/lib/apt/lists/* \
    # pgrx installs test artifacts into distro-owned directories. Grant this disposable,
    # single-purpose container's build user access; never apply this ownership change to a host.
    && PGC="/usr/lib/postgresql/${PG_MAJOR}/bin/pg_config" \
    && chown -R pgrx:pgrx "$("$PGC" --sharedir)/extension" "$("$PGC" --pkglibdir)"
USER pgrx

# pgrx-tests reads `$USER`; Docker's USER instruction does not export it.
ENV USER=pgrx

# Point pgrx at the PG this image already has rather than letting it compile another one.
RUN cargo pgrx init "--pg${PG_MAJOR}" "/usr/lib/postgresql/${PG_MAJOR}/bin/pg_config"

WORKDIR /work
COPY --chown=pgrx . .

# Ordinary test builds use the normalized Cargo package supplied as a named
# context. Release proof sets this to 0, leaving the plain version dependency to
# resolve exclusively from crates.io.
ARG KMONEY_USE_LOCAL_CORE=1
COPY --from=kamu-money-core --chown=pgrx:pgrx . /opt/kamu-money-core
RUN case "${KMONEY_USE_LOCAL_CORE}" in \
      0) echo "kamu-money-core: resolving from crates.io" ;; \
      1) printf '\n[patch.crates-io]\nkamu-money-core = { path = "/opt/kamu-money-core" }\n' \
           >> /work/.cargo/config.toml ;; \
      *) echo "KMONEY_USE_LOCAL_CORE must be 0 or 1" >&2; exit 2 ;; \
    esac
ENV KMONEY_USE_LOCAL_CORE=${KMONEY_USE_LOCAL_CORE}

# Repository configuration forces PGRX_HOME to `/work/.pgrx`; link it to the cached home created
# before source copy.
RUN ln -s /home/pgrx/.pgrx /work/.pgrx

# Exists (empty, pgrx-owned) so that a named cache volume mounted here by
# test-matrix.sh is seeded with THIS ownership. Docker seeds a fresh named
# volume from the image path's content and owner; without this the volume
# arrives root-owned and cargo cannot write to it.
RUN mkdir -p /work/target

ENV PG_MAJOR=${PG_MAJOR}
CMD ["bash", "-c", "exec cargo pgrx test --no-default-features --features \"pg${PG_MAJOR}\" \"pg${PG_MAJOR}\""]
