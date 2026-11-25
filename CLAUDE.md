# Prumo — instruções para o agente

## Convenções

- **Código em inglês**: arquivos, tipos, funções, variáveis, banco e campos
  JSON. Português só em comentário, `docs/`, `.md`, mensagens de commit/PR e no
  texto que a pessoa lê. Texto gerado no servidor vai sem acento (atravessa log);
  no navegador, com acento.
- **Commits em Conventional Commits**, em português: `tipo(escopo)!: assunto`
  (`feat fix perf refactor docs test build ci chore style revert`). O hook
  `commit-msg` vem de `core.hooksPath = D:/REPO/github-workflows/hooks` — não
  troque o `hooksPath`. A versão sai dos commits (`feat` → minor, `!` → major).
- **Repositório público**: nenhum dado real de carteira (posições, valores,
  totais) em teste, fixture, exemplo ou doc. Use carteira fictícia.
- **Sem débito técnico**: defeito achado no trabalho atual é consertado antes de
  fechar; contorno só com causa medida e escrita.
- **Pronto = no ar**: o trabalho termina com a versão nova respondendo em
  `https://prumo.alisonamorim.com/api/v1/info` com o `commit` recém-publicado.

## Onde está cada coisa

- `backend/` — Rust 1.98, Axum 0.8, sqlx 0.9 (consultas em tempo de execução,
  sem cache `.sqlx`). `domain/` é puro (o motor do aporte em
  `domain/allocation.rs`, explicado em `docs/alocacao.md`).
- `frontend/` — React 19 + Vite + TypeScript + Motion. Contrato em `docs/api.md`,
  desenho em `docs/design.md`.
- `deploy/values.yaml` — cópia de origem; quem vale é
  `homelab-gitops/apps/prumo/values.yaml` (o CI reescreve `image.tag` lá).

## Onde pôr código novo

O roteiro (`docs/roadmap.md`) é tocado por várias frentes ao mesmo tempo. Para
os ramos se juntarem sem briga:

- Backend: área nova em módulo próprio — `domain/<area>.rs` (regra pura),
  `store/<area>.rs` (`impl Store`), `http/<area>.rs` (handlers e DTOs da
  área). Nos arquivos comuns (`http/mod.rs`, `lib.rs`, `store/mod.rs`) só a
  linha que registra o módulo ou a rota.
- Migração nova: `NNNN_<area>.sql` com o próximo número livre na `main` no
  momento do merge (quem integra renumera se precisar).
- Frontend: chamadas em `api/<area>.ts` (usando `request` de `api/client.ts`),
  tipos em `api/<area>Types.ts`, telas em `features/<area>/`.

## Antes de abrir PR

```bash
cd backend  && cargo fmt --check && cargo clippy --all-targets --all-features -- -D warnings && cargo test
cd frontend && npm ci --ignore-scripts && npm run build && npm test
```

Os testes de integração do backend sobem Postgres 17 via testcontainers: o
Docker precisa estar ligado. A cobertura mínima da esteira é 80% nos dois.

Rodar local: ver `README.md`. Nesta máquina a 8080 tem dono (MoneyPrinterTurbo),
então suba o backend em outra porta e use `PRUMO_API=http://127.0.0.1:<porta>
npm run dev`.
