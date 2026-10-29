# Documentação de engenharia

Fonte documental do projeto kiosk (controle parental doméstico). Instruções para agentes em [../AGENTS.md](../AGENTS.md).

| Documento/pasta | Responsabilidade |
|---|---|
| [PENDING](PENDING.md) | Trabalho aberto, impedimentos e próximo passo; aponta para detalhes |
| [ROADMAP](ROADMAP.md) | Resultados desejados, sequência de sprints e dependências |
| [CONCLUSION](CONCLUSION.md) | Entregue, resolvido, descartado ou decidido, com evidência e commit |
| [architecture](architecture/README.md) | Arquitetura vigente, fronteiras e contratos |
| [epics](epics/README.md) | Frentes com resultado, fases e aceites |
| [gaps](gaps/README.md) | Obstáculos estruturais ou contratuais medidos |
| [adr](adr/ADR-INDEX.md) | Decisões, alternativas e consequências; aceite é do dono |
| [insights](insights/README.md) | Investigações, revisões e hipóteses ainda sem compromisso |
| [runbooks](runbooks/api-e-cli.md) | Procedimentos de operação (serviço, pareamento e CLI) |
| [templates](templates/README.md) | Modelos para novos documentos |

## Relações e manutenção

Um insight pode originar épico ou gap; um épico pode exigir decisões em ADRs. Cada documento liga sua origem e seus dependentes, e épicos e ADRs se referenciam nos dois sentidos.

IDs permanecem estáveis após conclusão. Mudar o estado não muda a categoria do arquivo. PENDING contém links e próximo passo; CONCLUSION registra o desfecho; ROADMAP registra direção, sem copiar a fila de bugs. GAP de Código vive só no PENDING. Reabertura é novo registro vinculado ao histórico.

Repositório único: esta raiz basta. Idioma: português (pt-BR).
