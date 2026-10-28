# Insight — Revisão do estado inicial e das sessões anteriores

Data: 2026-10-27. Autoria: Claude (Opus 5.5), a pedido do dono. Escopo: repositório `kiosk` em `420b258` + árvore de trabalho, e os três transcritos de sessão do projeto.

Questão investigada: o que ficou incorreto ou contraditório antes de qualquer implementação?

## Evidências

### Sessões anteriores

As duas primeiras sessões úteis rodaram num modelo local pequeno (`gemma4:e4b-mlx`).

| Sessão | Observado | Efeito |
|---|---|---|
| `5673d87b` | Sem mensagens | nenhum |
| `3a84a5f7` (2026-10-27, 22:02–22:49 UTC) | Gravou a regra "responder em português" num AGENTS global em `~/.claude/` | Arquivo global fora do projeto e não lido pelo Claude Code; a regra não tinha efeito. Agora está no [AGENTS.md](../../AGENTS.md) do projeto |
| `3a84a5f7` | Declarou o código "Conforme / Alta Maturidade" sem compilar | Falso: o manifesto nem resolve dependências (abaixo) |
| `3a84a5f7` | Afirmou que `src/database_schema.rs` "não foi fornecido" | Falso: o arquivo existe desde `420b258` |
| `63fbdc00` (primeira parte) | Plano de épicos genérico, montado sem ler a arquitetura | Substituído pelos [épicos](../epics/README.md) atuais |
| `63fbdc00` | Aceitou o relatório de hardening sem revisão | Ver [insight do relatório](2026-10-27-relatorio-hardening-windows.md) |
| `63fbdc00` | `git commit --amend` em `main` | Bloqueado pelo guard (`commit-protegida`); teria reescrito `420b258` |
| `63fbdc00` | Criou `EPICS_OVERVIEW` (raiz) e `src/data_models.rs` | Descartados (abaixo) |

### Repositório

- [ARCHITECTURE.md](../../ARCHITECTURE.md) e [docs/20260906133311.md](../20260906133311.md) eram **idênticos** (`cmp` sem diferença). A arquitetura passa para [architecture/README.md](../architecture/README.md); os dois caminhos antigos viram stubs.
- `cargo check --manifest-path src/Cargo.toml` (executado nesta sessão) falha no manifesto: `failed to select a version for the requirement uuid = "=1.9.2"`. O erro vem antes da compilação de qualquer fonte.
- Os defeitos de código abaixo foram vistos **por leitura**, sem compilar (o manifesto impede). Viraram GAPs de Código no [PENDING](../PENDING.md):
  - raw strings `r#"` fechadas com `"` sem `#`;
  - `#[derive]` aplicado a campo;
  - transação sem `commit()`;
  - nomes de tabela e coluna divergentes;
  - `SecurityEvent` definido duas vezes.
- `src/data_models.rs` criava um modelo paralelo (`UserProfile`, `AgeGroup`, `ConsentLevel`, `UsageRule`) sem relação com `database_schema.rs` nem com a arquitetura.

### Contradições entre a arquitetura e o produto

O produto foi definido pelo dono em 2026-10-27 como controle parental doméstico.

1. A "Regra da Discrepância" (Seção I.2) torna o enforcement passivo → [ADR-001](../adr/ADR-001-enforcement-ativo-e-visivel.md).
2. "Invisibilidade ao usuário alvo" → [ADR-001](../adr/ADR-001-enforcement-ativo-e-visivel.md).
3. Faltam perfil de criança e contabilidade de uso, e não há plataforma declarada → [ADR-002](../adr/ADR-002-identidade-por-conta-windows.md).
4. Rede (`100.x.x.x`) e esquema de assinatura sem decisão → hipóteses em [E-005](../epics/E-005-api-comando-e-pareamento.md).

## Alternativas e crítica

Reaproveitar `EPICS_OVERVIEW` e `data_models.rs` foi descartado: os dois ignoram o contrato da arquitetura e reintroduziriam o desalinhamento. O trabalho não commitado do dono em `src/` (`db_manager.rs`, `security_logger_impl.rs`, `Cargo.toml`) **não** foi tocado; ele é absorvido em [E-001](../epics/E-001-fundacao-crate-e-dados.md) F1.

## Encaminhamento

- Decididos pelo dono em 2026-10-27, por escolha direta:
  - bloqueio por app e por sessão;
  - agente visível;
  - somente Windows;
  - uma conta Windows por criança.

  Estão registrados como ADRs **Propostas** aguardando aceite formal.
- Continuam hipóteses: topologia de rede, esquema de assinatura, hash encadeado no log e política de retenção.
- Fica com o dono: remover o AGENTS global em `~/.claude/`, que está fora do repositório.
