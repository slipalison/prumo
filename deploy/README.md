# Como o Prumo chega ao cluster

Caminho dourado do homelab: imagem no GHCR → `homelab-gitops/apps/prumo/` →
ApplicationSet → ArgoCD → Rollout com canary. Este diretório guarda a cópia de
origem do `values.yaml`; quem o cluster lê é a do GitOps, e o CI reescreve
`image.tag` lá a cada push na `main`.

## A ordem (inverter custa caro)

1. **Porta fechada antes de abrir.** A regra `prumo.alisonamorim.com` entra na
   `AuthorizationPolicy` DENY do gateway
   (`homelab-gitops/platform/istio-gateway/manifests/authorizationpolicy-paineis.yaml`)
   **antes** do `values.yaml`. O CNAME curinga torna público todo
   `VirtualService` no instante em que sincroniza; com a regra primeiro, o host
   nasce negando quem não traz o token do Access.
2. **Aplicação no Cloudflare Access** (Zero Trust → Access → Applications →
   Self-hosted): domínio `prumo.alisonamorim.com`, política reutilizável
   `so-eu`. O AUD dela vai em dois lugares: a regra do passo 1 e
   `CF_ACCESS_AUD` no `values.yaml`.
3. **Credencial do banco**, no pve:

   ```bash
   /root/distros-setup/proxmox/setup-k8s-platform.sh --only pgapp --pg-app prumo
   /root/distros-setup/proxmox/setup-k8s-platform.sh --only pgfechar
   ```

   O primeiro sorteia a senha e sela nos dois namespaces; o segundo revoga o
   `CONNECT` de `PUBLIC` em todo banco (sem ele, um app entra no banco do
   outro).
4. **Primeira imagem publicada** pela esteira (push na `main` com `app: ""`).
5. **`apps/prumo/values.yaml` no GitOps**, com a tag da primeira imagem e o AUD.
6. **`app: prumo` no `ci.yml`** — dali em diante o CI escreve a tag sozinho.

## O que aconteceu no primeiro deploy (2026-11-25)

Passos 1, 3, 4, 5 e 6 feitos (homelab-gitops#47). O passo 2, a aplicação no
Access, ainda não existe: até lá a regra do gateway e o `CF_ACCESS_AUD` têm um
valor que não é AUD de nada, e o host responde **403** de fora (medido).

**Armadilha do chart `app` 0.8.0 em app novo:** o `Database` vem na onda 1 e o
`Rollout` na onda 0. O ArgoCD espera o Rollout ficar saudável antes da onda 1,
e o Rollout não fica saudável sem o banco — impasse (`waiting for healthy state
of argoproj.io/Rollout/prumo`, com o pod em `database "prumo" does not exist`).
Os apps antigos não viram porque o banco deles já existia. Destravado aplicando
uma vez, à mão, o `Database` renderizado pelo próprio chart; o ArgoCD o adotou.
A correção é no `slipalison/helm-charts` (role e banco antes do workload).

## Como saber que está no ar

```bash
curl -sI https://prumo.alisonamorim.com            # 302 para o Access
```

Logado no navegador, `https://prumo.alisonamorim.com/api/v1/info` responde a
versão e o `commit` — que tem de bater com o SHA recém-publicado, senão o teste
está medindo o pod anterior.
