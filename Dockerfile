# syntax=docker/dockerfile:1.7
#
# Uma imagem só: o binário Rust serve a API e os estáticos do PWA.
# Mesmo desenho do basalto e de `exemplos/ci-monorepo-back-front.yml` do
# slipalison/github-workflows.

# ------------------------------------------------------------- frontend ----
FROM node:24-alpine AS frontend
WORKDIR /build
COPY frontend/package.json frontend/package-lock.json ./
# Sem scripts de instalação: é o vetor clássico de pacote npm comprometido, e
# nenhuma dependência daqui precisa deles.
RUN npm ci --ignore-scripts
COPY frontend/ ./
RUN npm run build

# -------------------------------------------------------------- backend ----
FROM rust:1.98-slim-bookworm AS backend
WORKDIR /build
# Dependências primeiro, numa camada própria: mudar código não recompila o
# mundo. O `main.rs`/`lib.rs` vazios só existem para o cargo resolver o grafo.
COPY backend/Cargo.toml backend/Cargo.lock ./
RUN mkdir src \
    && echo 'fn main() {}' > src/main.rs \
    && echo '' > src/lib.rs \
    && cargo build --release --locked \
    && rm -rf src
COPY backend/src ./src
COPY backend/migrations ./migrations
COPY backend/build.rs ./build.rs
RUN touch src/main.rs src/lib.rs && cargo build --release --locked

# -------------------------------------------------------------- runtime ----
FROM gcr.io/distroless/cc-debian12:nonroot
# A esteira passa os dois como build-arg; o chart reescreve APP_VERSION em
# tempo de execução, e o GIT_SHA só existe aqui.
ARG APP_VERSION=0.0.0-dev
ARG GIT_SHA=unknown
ENV APP_VERSION=${APP_VERSION} \
    GIT_SHA=${GIT_SHA} \
    STATIC_DIR=/static \
    PORT=8080 \
    OPERATIONAL_PORT=9090
COPY --from=backend  /build/target/release/prumo /prumo
COPY --from=frontend /build/dist                 /static
USER 65532:65532
EXPOSE 8080 9090
ENTRYPOINT ["/prumo"]
