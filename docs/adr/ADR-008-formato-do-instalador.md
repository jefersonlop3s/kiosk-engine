# ADR-008 — Formato do instalador

Status: Aceito em 2026-10-29 pelo dono (Jeferson Lopes). Data: 2026-10-29. Autoridade de aceite: dono do projeto (Jeferson Lopes).
Repositórios/contratos afetados: kiosk; [E-007](../epics/E-007-onboarding-e-instalador.md) F2 e F3; GC-16 do [PENDING](../PENDING.md).

## Contexto

Hoje o kiosk é um `.zip` com dois executáveis, e o `kioskd servir` roda num console aberto (GC-16). Para uso real, o serviço precisa iniciar com a máquina como SYSTEM, se recuperar sozinho se cair, ter a pasta do banco protegida e poder ser removido sem deixar rastro. O [E-007](../epics/E-007-onboarding-e-instalador.md) prevê essas entregas, mas não definiu o formato.

## Decisão

Proposta, em duas etapas:

1. **Etapa 1 (F2): autoinstalação pelo próprio executável.**
   - `kioskd instalar` copia os executáveis para `C:\Program Files\kiosk`, cria `C:\ProgramData\kiosk` com permissão só para SYSTEM e Administradores, registra o serviço no SCM (conta SYSTEM, início automático, reinício em caso de falha) e o inicia.
   - `kioskd desinstalar` para e remove o serviço e os executáveis. O banco só é apagado com `--apagar-dados`, depois de confirmação.
   - Testável no runner Windows do CI, que tem administrador.
2. **Etapa 2 (F3): pacote `.msi`** gerado com WiX (`cargo-wix`) que faz o mesmo que a etapa 1, mais atualização e desinstalação pelo Painel de Controle; publicado também no **winget**.
3. **Assinatura de código**: sem certificado, o Windows SmartScreen alerta ao baixar e executar. Fica registrado como risco; a compra de um certificado é decisão do dono e não bloqueia a etapa 1.

Alternativas rejeitadas:
- **MSIX**: o empacotamento isola o app e dificulta um serviço SYSTEM que encerra processos de outros usuários.
- **NSIS ou Inno Setup**: funcionam, mas são menos integrados ao Windows que o MSI (atualização, política de grupo) e não trazem vantagem sobre o `.msi`.
- **Só winget**: o winget distribui um instalador; ainda precisaria de um `.msi` ou de um `.exe` instalador por baixo.
- **Manter o `.zip` com console aberto**: é o estado atual; não sobrevive a reinício nem protege o serviço.

## Consequências

- Ganho: a etapa 1 não exige ferramenta nova e reaproveita o código; o serviço passa a sobreviver a reinícios.
- Custo: código Win32 de SCM e ACL; o `.msi` traz a toolchain do WiX para o CI.
- Risco: um serviço SYSTEM mal configurado amplia o impacto de falhas; a criança não pode conseguir parar o serviço (antiadulteração do [E-004](../epics/E-004-agente-windows-enforcement.md) F4).
- Reversível: a etapa 2 empacota a etapa 1.
- Validação: CI Windows instalando, iniciando, reiniciando e removendo o serviço; teste negativo de que uma conta padrão não consegue parar o serviço.

## Referências e evolução

- Consome: [E-007](../epics/E-007-onboarding-e-instalador.md) F2 e F3; resolve o GC-16.
- Relaciona-se com: [ADR-004](ADR-004-comunicacao-servico-ui-por-named-pipe.md) (o serviço inicia a UI na sessão), [ADR-007](ADR-007-forma-do-assistente-de-configuracao.md).
