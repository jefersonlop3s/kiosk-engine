# E-005 — API de comando e pareamento

Estado: em andamento (F1, F2 e o núcleo da F3 prontos em 2026-10-28; falta o CLI dos pais). Responsável: a definir. Repositório: kiosk.

## Resultado e escopo

**Resultado observável**: um dispositivo dos pais previamente pareado cria, altera e revoga regras e consulta uso e log pela API local do serviço. Toda requisição passa pelas Camadas 0–5 da arquitetura e responde de forma genérica (`202 Accepted` ou erro sem detalhe).

- **Inclui**: pareamento (troca de chave pública numa ação presencial na conta de admin), assinatura das requisições (*proof-of-possession*), autorização por papel, validação de parâmetros, transição atômica de `Active_Rules` com evento no log, e um cliente CLI mínimo para os pais (substituto do app mobile no MVP).
- **Exclui**: o app mobile ([E-006](E-006-app-mobile-administracao.md)).

## Origem e decisões

- [Arquitetura](../architecture/README.md), Seção III (Camadas 0–5) e `POST /api/windows/{action}`.
- **ADRs propostas (2026-10-28), aguardando aceite do dono**:
  - [ADR-005](../adr/ADR-005-rede-da-api-de-comando.md): rede privada Tailscale por padrão, LAN opcional com HTTPS, nunca exposta à internet;
  - [ADR-006](../adr/ADR-006-assinatura-de-requisicoes-e-pareamento.md): Ed25519 por dispositivo, anti-replay por `timestamp` e `nonce`, pareamento presencial.

## Fases e dependências

| Fase | Entrega | Depende de | Evidência |
|---|---|---|---|
| F1 | Servidor HTTP no serviço, Camadas 0 e 3 (recepção e validação) | [E-001](E-001-fundacao-crate-e-dados.md), [E-002](E-002-log-forense.md) F1, ADR de rede | Testes HTTP reais contra o serviço |
| F2 | Pareamento e Camadas 1 e 2 (autenticação e autorização) | F1, ADR de assinatura | Teste negativo: requisição sem assinatura, com assinatura inválida ou repetida (replay) gera `AUTH_FAIL` |
| F3 | Camadas 4 e 5 + CLI dos pais | F2, [E-003](E-003-motor-de-regras-e-uso.md) | Ciclo completo criar → ativar → revogar via CLI |

## Entregue (2026-10-28)

- F2, núcleo sem servidor HTTP: `assinatura` (string canônica `KIOSK-V1`, Ed25519 estrito, nonce, código de pareamento) e `pareamento` (Camada 1 `autenticar`, Camada 2 `autorizar`, pareamento presencial por código de uso único, revogação). Migração 0006: `paired_device`, `seen_nonce`, `pairing_code`.
- Recusas respondem com `Rejeicao` genérica; o motivo exato vai para `AUTH_FAIL`/`AUTHZ_FAIL`/`VALIDATION_FAIL`.
- F1 e núcleo da F3: `api` (axum) com `POST /api/windows/{ação}` atravessando as Camadas 0 a 5; `comandos` valida os parâmetros (esquema fechado, limites, horários `HH:MM`, referências existentes); ações `add_rule` (nasce `PENDING`), `activate_rule` e `revoke_rule`, cada uma com o evento de sucesso na mesma transação. Respostas: `202 {"status":"accepted"}` (com `id` ao criar), `403 {"status":"rejected"}` idêntico para qualquer recusa e `500` genérico; corpo máximo de 16 KiB.
- Política de rede da ADR-005 aplicada em código: `vincular` só abre loopback e Tailscale (`100.64.0.0/10`, `fd7a:115c:a1e0::/48`) e recusa curinga e LAN antes de abrir qualquer porta.
- Faltam: CLI dos pais, `modify_limit`, consultas de uso e log, e o `main` do serviço (GC-14 a GC-16 no [PENDING](../PENDING.md)).

## Aceites

- Nenhuma resposta externa vaza `rejection_reason_detail`.
- Sem superfície linguística na API; mensagens do CLI em pt-BR.

## Encerramento

A preencher.
