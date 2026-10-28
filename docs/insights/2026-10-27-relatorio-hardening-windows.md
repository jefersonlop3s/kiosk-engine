# Insight — Relatório "Hardening de Sistema Windows: Políticas GPO e Anti-Bypass"

Data: 2026-10-27. Autoria: Claude (Opus 5.5). Escopo: relatório colado na conversa de 2026-10-27, assinado como "Agente Executor Cíbrido". O texto não está versionado no repositório.

Questão: o relatório serve de base para o anti-bypass do agente ([E-004](../epics/E-004-agente-windows-enforcement.md))?

## Evidências e hipóteses

Revisão **por leitura**: nenhum comando do relatório foi executado em Windows. O relatório mira hardening corporativo (GPO de domínio, SOC, CISO), não um PC doméstico. Problemas encontrados:

| Item do relatório | Problema |
|---|---|
| Desabilitar o serviço `TrustedInstaller` | Ele instala as atualizações do Windows; desabilitá-lo compromete o Windows Update |
| `RestrictSoftwareInstallForUserAccounts` = `0` | A chave não é documentada pela Microsoft que eu conheça, e o valor `0` contradiz o objetivo declarado (restringir) |
| `SecurityModeLsaProtectionLevel`, `New-PSDrive ... Name LocalSecurityAuthorityRegistryType` | Chaves e parâmetros que não correspondem ao mecanismo real de proteção do LSA (`RunAsPPL`); o script tem sintaxe quebrada |
| `netsh advfirewall set allprofiles state BLOCK_INBOUND_CONNECTIONS=Yes` | Sintaxe inválida para `netsh` |
| `New-NetFirewallRule -LocalPort TCP,22` | Parâmetro inválido; e abrir SSH de entrada não tem função num PC de criança |
| Regra de firewall "BlockShadowCopy" contra DLL injection | Firewall não controla Volume Shadow Copy nem injeção de DLL |
| Script consolidado (Seção 4) | Termina no meio da instrução |
| Final do documento | Parágrafo longo e repetitivo, sem conteúdo técnico |

Hipótese (não verificada): o relatório foi gerado por um modelo sem validação e contém comandos plausíveis porém incorretos. **Nada dele deve ser executado.**

## Alternativas e crítica

Ideias corretas que coincidem com decisões já tomadas, aproveitadas como *requisito*, não como comando:

- **A criança usa conta padrão, sem admin** ([ADR-002](../adr/ADR-002-identidade-por-conta-windows.md)). É a principal barreira de anti-bypass num PC doméstico.
- **O serviço não pode ser parado por não-admin**, com recuperação automática e alerta ([E-004](../epics/E-004-agente-windows-enforcement.md) F4).
- **Log de tentativas** ([E-002](../epics/E-002-log-forense.md)).

Fora de escopo: GPO de domínio (muitos PCs domésticos rodam Windows Home, sem editor de políticas de grupo), firewall de rede, SOC e LSA.

## Encaminhamento

Relatório **não adotado**. Os requisitos aproveitáveis já estão em E-004 F4. Qualquer configuração de SO que o agente aplicar terá ADR própria e validação em Windows real.
